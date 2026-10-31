//! ECMAScript function objects, calls, constructors, and parameter binding.

mod constructor_assignments;
mod super_call;

use super::{
    environment::StoredEnvironment,
    execution::StoredRealm,
    stored::{StoredObject, StoredValue},
};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
    ops::Deref,
    rc::{Rc, Weak},
};

use crate::ast::{Expr, ExprKind, Function, FunctionBody, MethodKind, StmtKind};
use crate::bytecode::{BytecodeCompiler, FunctionTemplate};

use super::{
    Completion, Environment, ExpressionEvaluator, JsString, Object, PropertyKey, Realm,
    RuntimeError, RuntimeResult, StatementEvaluator, ThisBindingStatus, Value,
    generator::create_generator_object, promise::settled_promise_object,
};

/// Runtime behavior selected for a parsed function definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionKind {
    /// An ordinary function with dynamic `this` and `[[Construct]]`.
    Normal,
    /// An arrow function with lexical `this` and no `[[Construct]]`.
    Arrow,
    /// An object or class method.
    Method(MethodKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConstructorKind {
    Base,
    Derived,
    DefaultDerived,
}

type FunctionCacheKey = (usize, usize, u8);
type CodeCache = HashMap<FunctionCacheKey, Vec<Rc<FunctionCode>>>;
pub(super) type FunctionCache = Rc<RefCell<CodeCache>>;

#[derive(Debug)]
pub(super) struct FunctionCode {
    ast: Function,
    kind: FunctionKind,
    compiled: Option<Rc<FunctionTemplate>>,
    lexical_property: Option<PropertyKey>,
    constructor_assignments: Option<constructor_assignments::Plan>,
    may_observe_arguments: bool,
    delegated_parameters: Option<Box<[usize]>>,
    super_call: Option<super_call::Plan>,
    auto_functions: bool,
    cache: Weak<RefCell<CodeCache>>,
}
impl Deref for FunctionCode {
    type Target = Function;
    fn deref(&self) -> &Function {
        &self.ast
    }
}
impl FunctionCode {
    fn context(&self) -> Option<FunctionCache> {
        self.auto_functions
            .then(|| self.cache.upgrade().unwrap_or_default())
    }
}

fn prepare_code(
    code: &Function,
    kind: FunctionKind,
    cache: &Option<FunctionCache>,
) -> Rc<FunctionCode> {
    let tag = match kind {
        FunctionKind::Normal => 0,
        FunctionKind::Arrow => 1,
        FunctionKind::Method(MethodKind::Method) => 2,
        FunctionKind::Method(MethodKind::Get) => 3,
        FunctionKind::Method(MethodKind::Set) => 4,
        FunctionKind::Method(MethodKind::Constructor) => 5,
    };
    let key = (code.span.start, code.span.end, tag);
    if let Some(cache) = cache {
        if let Some(found) = cache.borrow().get(&key).and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.kind == kind && entry.ast == *code)
        }) {
            return found.clone();
        }
    }
    let compiled = if cache.is_some() && kind != FunctionKind::Arrow {
        BytecodeCompiler::compile_function_template(code)
            .ok()
            .map(Rc::new)
    } else {
        None
    };
    let prepared = Rc::new(FunctionCode {
        ast: code.clone(),
        kind,
        compiled,
        constructor_assignments: if cache.is_some() {
            constructor_assignments::Plan::prepare(code, kind)
        } else {
            None
        },
        lexical_property: if cache.is_some() {
            lexical_property(code, kind)
        } else {
            None
        },
        may_observe_arguments: may_observe_arguments(code),
        delegated_parameters: if kind == FunctionKind::Method(MethodKind::Constructor) {
            delegated_parameters(code)
        } else {
            None
        },
        super_call: super_call::Plan::prepare(code, kind),
        auto_functions: cache.is_some(),
        cache: cache.as_ref().map_or_else(Weak::new, Rc::downgrade),
    });
    if let Some(cache) = cache {
        cache
            .borrow_mut()
            .entry(key)
            .or_default()
            .push(prepared.clone());
    }
    prepared
}

// A zero-parameter synchronous arrow with only a property read cannot expose
// an activation. Keep getter/Proxy dispatch and lexical-this TDZ resolution.
fn lexical_property(code: &Function, kind: FunctionKind) -> Option<PropertyKey> {
    if kind != FunctionKind::Arrow
        || code.asynchronous
        || code.generator
        || !code.parameters.is_empty()
    {
        return None;
    }
    let FunctionBody::Expression(expression) = &code.body else {
        return None;
    };
    let ExprKind::Member {
        object,
        property,
        computed: false,
        optional: false,
    } = &expression.kind
    else {
        return None;
    };
    let ExprKind::Identifier(name) = &property.kind else {
        return None;
    };
    matches!(object.kind, ExprKind::This).then(|| PropertyKey::from(name.as_str()))
}

// A sole super(p, q, ...) with simple parameters cannot observe or retain its
// activation. Preserve the explicit argument List (including missing values,
// reordering and dropped extra actuals), rather than forwarding all actuals.
fn delegated_parameters(function: &Function) -> Option<Box<[usize]>> {
    if !function.strict || function.asynchronous || function.generator {
        return None;
    }
    let FunctionBody::Block(body) = &function.body else {
        return None;
    };
    let StmtKind::Block(statements) = &body.kind else {
        return None;
    };
    let [statement] = statements.as_slice() else {
        return None;
    };
    let StmtKind::Expression(expression) = &statement.kind else {
        return None;
    };
    let ExprKind::Call {
        callee,
        arguments,
        optional: false,
    } = &expression.kind
    else {
        return None;
    };
    if !matches!(callee.kind, ExprKind::Super) {
        return None;
    }
    let mut parameters = HashMap::with_capacity(function.parameters.len());
    for (index, parameter) in function.parameters.iter().enumerate() {
        let ExprKind::Identifier(name) = &parameter.kind else {
            return None;
        };
        if name == "arguments"
            || name == "eval"
            || parameters.insert(name.as_str(), index).is_some()
        {
            return None;
        }
    }
    arguments
        .iter()
        .map(|argument| {
            let ExprKind::Identifier(name) = &argument.kind else {
                return None;
            };
            parameters.get(name.as_str()).copied()
        })
        .collect::<Option<Vec<_>>>()
        .map(Vec::into_boxed_slice)
}

// Scan complete syntax once, including defaults, arrows, nested classes and
// functions. Over-approximating nested scopes is safe. Keep the binding for
// direct eval as well, independently of whether eval is currently supported.
fn may_observe_arguments(function: &Function) -> bool {
    let mut observed = false;
    function.visit_expressions(&mut |expression| {
        if let ExprKind::Identifier(name) = &expression.kind {
            observed |= name == "arguments" || name == "eval";
        }
    });
    observed
}

#[derive(Clone)]
pub(super) struct FunctionClosure {
    pub(super) code: Rc<FunctionCode>,
    pub(super) kind: FunctionKind,
    pub(super) realm: Realm,
    pub(super) environment: Environment,
    pub(super) lexical_this: Value,
    lexical_this_environment: Option<Environment>,
    constructor_kind: ConstructorKind,
}

pub(super) struct StoredFunctionClosure {
    code: Rc<FunctionCode>,
    kind: FunctionKind,
    realm: StoredRealm,
    environment: StoredEnvironment,
    lexical_this: StoredValue,
    lexical_this_environment: Option<StoredEnvironment>,
    constructor_kind: ConstructorKind,
    home_object: Option<StoredObject>,
    immutable_class_prototype: bool,
}
impl StoredFunctionClosure {
    pub(super) fn super_base(&self, owner: &Object) -> RuntimeResult<Value> {
        let home = self
            .home_object
            .as_ref()
            .ok_or_else(|| RuntimeError::reference_error("function has no super binding"))?
            .load(owner)?;
        Ok(home.try_prototype()?.map_or(Value::Null, Value::Object))
    }

    pub(super) fn has_legacy_properties(&self) -> bool {
        self.kind == FunctionKind::Normal
            && !self.code.strict
            && !self.code.generator
            && !self.code.asynchronous
    }
    pub(super) fn source_realm(&self) -> &StoredRealm {
        &self.realm
    }
    // The live function argument (or its stored generator environment) roots
    // HomeObject through this stored closure. The call facade never uses it;
    // super resolves the traced edge from the owning function environment.
    fn load(&self, owner: &Object) -> RuntimeResult<FunctionClosure> {
        Ok(FunctionClosure {
            code: self.code.clone(),
            kind: self.kind,
            constructor_kind: self.constructor_kind,
            lexical_this_environment: self
                .lexical_this_environment
                .as_ref()
                .map(|environment| environment.load(owner))
                .transpose()?,
            realm: self.realm.load(owner)?,
            environment: self.environment.load(owner)?,
            // Verified ordinary/method bodies use their prepared this binding;
            // direct super operations remain AST. Nested class evaluation resolves
            // super through the function environment and stored HomeObject.
            // Retain stored GC edges without reviving unused call-local roots.
            lexical_this: if self.code.compiled.is_some() {
                Value::Undefined
            } else {
                self.lexical_this.load(owner)?
            },
        })
    }
    pub(super) fn is_constructor(&self) -> bool {
        !self.code.generator
            && !self.code.asynchronous
            && matches!(
                self.kind,
                FunctionKind::Normal | FunctionKind::Method(MethodKind::Constructor)
            )
    }
    pub(super) fn call(
        &self,
        owner: &Object,
        this: Value,
        arguments: &[Value],
    ) -> RuntimeResult<Value> {
        if self.kind == FunctionKind::Method(MethodKind::Constructor) {
            return Err(self
                .realm
                .load(owner)?
                .bind_exception(RuntimeError::type_error("class constructor requires new")));
        }
        if let Some(plan) = &self.code.super_call {
            let realm = self.realm.load(owner)?;
            return plan
                .execute(self, owner, this, arguments, &realm)
                .map_err(|error| realm.bind_exception(error));
        }
        if let Some(key) = &self.code.lexical_property {
            let realm = self.realm.load(owner)?;
            let receiver = match &self.lexical_this_environment {
                Some(environment) => environment.load(owner)?.get_this_binding(),
                None => self.lexical_this.load(owner),
            }
            .map_err(|error| realm.bind_exception(error))?;
            if let Value::Object(receiver) = receiver {
                return receiver
                    .get(key)
                    .map_err(|error| realm.bind_exception(error));
            }
            // Primitive/null/undefined receivers retain normal coercion/errors.
        }
        if let Some(template) = &self.code.compiled {
            return super::bytecode_function::execute_stored_template(
                template,
                &self.realm,
                &self.environment,
                owner,
                this,
                arguments,
            );
        }
        let closure = self.load(owner)?;
        if closure.code.generator {
            create_generator_object(&closure, owner.clone(), this, arguments).map(Value::Object)
        } else if closure.code.asynchronous {
            settled_promise_object(
                &closure.realm,
                call_function(&closure, owner.clone(), this, arguments, None)
                    .map_err(|error| closure.realm.bind_exception(error)),
            )
            .map(Value::Object)
        } else {
            call_function(&closure, owner.clone(), this, arguments, None)
                .map_err(|error| closure.realm.bind_exception(error))
        }
    }
    pub(super) fn construct(
        &self,
        owner: &Object,
        arguments: &[Value],
        new_target: Object,
    ) -> RuntimeResult<Object> {
        // Only direct construction of a base class can use its immutable own
        // prototype. Alternate new targets (including Proxy wrappers) retain
        // GetPrototypeFromConstructor and its observable property access.
        let prototype = if self.immutable_class_prototype && owner == &new_target {
            self.home_object
                .as_ref()
                .map(|object| object.load(owner))
                .transpose()?
        } else {
            None
        };
        let closure = self.load(owner)?;
        construct_function(&closure, owner.clone(), arguments, new_target, prototype)
    }
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        self.realm.trace(tracer);
        self.environment.trace(tracer);
        self.lexical_this.trace(tracer);
        if let Some(environment) = &self.lexical_this_environment {
            environment.trace(tracer);
        }
        if let Some(object) = &self.home_object {
            object.trace(tracer);
        }
    }
}

pub(super) fn create_function(
    code: &Function,
    kind: FunctionKind,
    evaluator: &ExpressionEvaluator,
    home_object: Option<Object>,
) -> RuntimeResult<Object> {
    create_function_with_constructor(
        code,
        kind,
        evaluator,
        home_object,
        ConstructorKind::Base,
        None,
        None,
    )
}

pub(super) fn create_function_with_constructor(
    code: &Function,
    kind: FunctionKind,
    evaluator: &ExpressionEvaluator,
    home_object: Option<Object>,
    constructor_kind: ConstructorKind,
    constructor_parent: Option<Object>,
    class_name: Option<JsString>,
) -> RuntimeResult<Object> {
    if code.asynchronous && code.generator {
        return Err(RuntimeError::internal_error(
            "async generators are not yet implemented",
        ));
    }
    let name_environment =
        if let Some(ExprKind::Identifier(name)) = code.name.as_ref().map(|name| &name.kind) {
            let environment = Environment::declarative(evaluator.environment.clone());
            environment.create_immutable_binding(JsString::from(name.as_str()), false)?;
            Some(environment)
        } else {
            None
        };
    let function = Object::new_with_prototype(constructor_parent.unwrap_or_else(|| {
        evaluator
            .realm
            .intrinsic(super::IntrinsicId::FunctionPrototype)
    }));
    let closure = StoredFunctionClosure {
        code: prepare_code(code, kind, &evaluator.function_cache),
        kind,
        realm: StoredRealm::store(&function, &evaluator.realm)?,
        environment: StoredEnvironment::store(
            &function,
            name_environment.as_ref().unwrap_or(&evaluator.environment),
        )?,
        lexical_this: StoredValue::store(&function, &evaluator.this_value)?,
        lexical_this_environment: if kind == FunctionKind::Arrow {
            evaluator
                .this_environment
                .as_ref()
                .map(|environment| StoredEnvironment::store(&function, environment))
                .transpose()?
        } else {
            None
        },
        constructor_kind,
        // The class evaluator publishes this function only after defining its
        // non-writable, non-configurable prototype as this same HomeObject.
        // Other AST-created functions never gain this construction shortcut.
        immutable_class_prototype: class_name.is_some()
            && kind == FunctionKind::Method(MethodKind::Constructor)
            && constructor_kind == ConstructorKind::Base,
        home_object: home_object
            .as_ref()
            .map(|object| StoredObject::store(&function, object))
            .transpose()?,
    };
    let constructable = closure.is_constructor();
    function.set_managed_callable(super::object::ManagedCallable::Ast(closure));
    if let (Some(environment), Some(ExprKind::Identifier(name))) =
        (name_environment, code.name.as_ref().map(|name| &name.kind))
    {
        environment.initialize_binding(
            &JsString::from(name.as_str()),
            Value::Object(function.clone()),
        )?;
    }

    let length = code
        .parameters
        .iter()
        .take_while(|parameter| {
            !matches!(
                parameter.kind,
                ExprKind::Assignment { .. } | ExprKind::Spread(_)
            )
        })
        .count();
    if let Some(name) = class_name {
        // Class functions are still fresh ordinary objects. Their complete
        // standard metadata can be stored without repeatedly validating and
        // reviving property descriptors. No user code runs before publication.
        function.initialize_function_properties(length, name, None)?;
    } else {
        function.define_own_property(
            "length".into(),
            super::PropertyDescriptor::data(Value::Number(length as f64), false, false, true),
        )?;
        if let Some(ExprKind::Identifier(name)) = code.name.as_ref().map(|name| &name.kind) {
            function.create_data_property(
                "name".into(),
                Value::String(JsString::from(name.as_str())),
            )?;
        }
    }
    if constructable && kind != FunctionKind::Method(MethodKind::Constructor) {
        function.install_lazy_function_prototype(
            evaluator
                .realm
                .intrinsic(super::IntrinsicId::ObjectPrototype),
        )?;
    }
    Ok(function)
}

fn call_function(
    closure: &FunctionClosure,
    function: Object,
    this_argument: Value,
    arguments: &[Value],
    new_target: Option<Object>,
) -> RuntimeResult<Value> {
    if let Some(template) = &closure.code.compiled {
        return super::bytecode_function::execute_template(
            template.clone(),
            closure.realm.clone(),
            closure.environment.clone(),
            function,
            this_argument,
            arguments,
            new_target,
        );
    }
    let lexical = closure.kind == FunctionKind::Arrow;
    let this_value = if lexical {
        closure.lexical_this.clone()
    } else if closure.code.strict {
        this_argument
    } else {
        ordinary_this(&closure.realm, this_argument)?
    };
    let environment = Environment::function(
        Some(closure.environment.clone()),
        function,
        new_target.clone(),
        if lexical {
            ThisBindingStatus::Lexical
        } else {
            ThisBindingStatus::Uninitialized
        },
    );
    if !lexical {
        environment.bind_this_value(this_value.clone())?;
    }
    // The complete syntax scan includes defaults, nested classes/functions,
    // arrows and direct eval. Retain the actual object whenever any of those
    // can observe arguments; otherwise this implicit binding is unreachable.
    if !lexical && closure.code.may_observe_arguments {
        bind_arguments(&closure.realm, &environment, arguments)?;
    }
    let evaluator = ExpressionEvaluator::with_environment(
        closure.realm.clone(),
        environment.clone(),
        this_value.clone(),
        closure.code.strict,
    )
    .with_new_target(new_target.clone())
    .with_this_environment(closure.lexical_this_environment.clone())
    .with_function_cache(closure.code.context());
    bind_parameters(
        &evaluator,
        &environment,
        &closure.code.parameters,
        arguments,
    )?;

    match &closure.code.body {
        FunctionBody::Expression(expression) => evaluator.evaluate(expression),
        FunctionBody::Block(body) => {
            let statements = StatementEvaluator::with_environments(
                closure.realm.clone(),
                environment.clone(),
                environment,
                this_value,
                closure.code.strict,
            )
            .with_new_target(new_target)
            .with_this_environment(closure.lexical_this_environment.clone())
            .with_function_cache(evaluator.function_cache.clone());
            match statements.evaluate_statement(body)? {
                Completion::Return(value) => Ok(value),
                Completion::Normal(_) => Ok(Value::Undefined),
                Completion::Throw(value) => Err(RuntimeError::thrown(value)),
                Completion::Break { .. } | Completion::Continue { .. } => Err(
                    RuntimeError::internal_error("invalid completion escaped a function body"),
                ),
            }
        }
    }
}

fn construct_function(
    closure: &FunctionClosure,
    function: Object,
    arguments: &[Value],
    new_target: Object,
    prototype: Option<Object>,
) -> RuntimeResult<Object> {
    if closure.constructor_kind != ConstructorKind::Base {
        return construct_derived(closure, function, arguments, new_target)
            .map_err(|error| closure.realm.bind_exception(error));
    }
    let prototype = match prototype {
        Some(prototype) => prototype,
        None => new_target.constructor_prototype(super::IntrinsicId::ObjectPrototype, || {
            Ok(closure.realm.intrinsic(super::IntrinsicId::ObjectPrototype))
        })?,
    };
    let instance = Object::new_for(&new_target);
    instance.initialize_prototype(Some(prototype));
    function.initialize_instance_fields(&instance)?;
    if let Some(plan) = &closure.code.constructor_assignments {
        plan.execute(&instance, arguments)
            .map_err(|error| closure.realm.bind_exception(error))?;
        return Ok(instance);
    }
    let result = call_function(
        closure,
        function,
        Value::Object(instance.clone()),
        arguments,
        Some(new_target),
    )
    .map_err(|error| closure.realm.bind_exception(error))?;
    match result {
        Value::Object(object) => Ok(object),
        _ => Ok(instance),
    }
}

fn construct_derived(
    closure: &FunctionClosure,
    function: Object,
    arguments: &[Value],
    new_target: Object,
) -> RuntimeResult<Object> {
    if closure.constructor_kind == ConstructorKind::DefaultDerived {
        // Forward the argument List directly, without an observable array iterator.
        let parent = function
            .try_prototype()?
            .filter(Object::is_constructor)
            .ok_or_else(|| RuntimeError::type_error("super constructor is not a constructor"))?;
        let instance = parent.construct(arguments, new_target)?;
        function.initialize_instance_fields(&instance)?;
        return Ok(instance);
    }
    if let Some(parameters) = &closure.code.delegated_parameters {
        // No this/arguments/eval, defaults, spread, closures or later statements
        // can observe BindThisValue in this shape. Initialize own fields after
        // the parent returns, before exposing the initialized receiver.
        let parent = function.try_prototype()?;
        let arguments: Cow<'_, [Value]> = if arguments.len() >= parameters.len()
            && parameters.iter().copied().eq(0..parameters.len())
        {
            Cow::Borrowed(&arguments[..parameters.len()])
        } else {
            Cow::Owned(
                parameters
                    .iter()
                    .map(|index| arguments.get(*index).cloned().unwrap_or(Value::Undefined))
                    .collect(),
            )
        };
        let parent = parent
            .filter(Object::is_constructor)
            .ok_or_else(|| RuntimeError::type_error("super constructor is not a constructor"))?;
        let instance = parent.construct(&arguments, new_target)?;
        function.initialize_instance_fields(&instance)?;
        return Ok(instance);
    }
    let environment = Environment::function(
        Some(closure.environment.clone()),
        function,
        Some(new_target.clone()),
        ThisBindingStatus::Uninitialized,
    );
    if closure.code.may_observe_arguments {
        bind_arguments(&closure.realm, &environment, arguments)?;
    }
    let evaluator = ExpressionEvaluator::with_environment(
        closure.realm.clone(),
        environment.clone(),
        Value::Undefined,
        true,
    )
    .with_new_target(Some(new_target.clone()))
    .with_this_environment(Some(environment.clone()))
    .with_function_cache(closure.code.context());
    bind_parameters(
        &evaluator,
        &environment,
        &closure.code.parameters,
        arguments,
    )?;
    let FunctionBody::Block(body) = &closure.code.body else {
        return Err(RuntimeError::internal_error(
            "derived constructor requires a block body",
        ));
    };
    let statements = StatementEvaluator::with_environments(
        closure.realm.clone(),
        environment.clone(),
        environment.clone(),
        Value::Undefined,
        true,
    )
    .with_new_target(Some(new_target))
    .with_this_environment(Some(environment.clone()))
    .with_function_cache(evaluator.function_cache.clone());
    let result = match statements.evaluate_statement(body)? {
        Completion::Return(value) => value,
        Completion::Normal(_) => Value::Undefined,
        Completion::Throw(value) => return Err(RuntimeError::thrown(value)),
        Completion::Break { .. } | Completion::Continue { .. } => {
            return Err(RuntimeError::internal_error(
                "invalid completion escaped a constructor",
            ));
        }
    };
    match result {
        Value::Object(object) => Ok(object),
        Value::Undefined => match environment.get_this_binding()? {
            Value::Object(object) => Ok(object),
            _ => Err(RuntimeError::internal_error(
                "constructor this is not an object",
            )),
        },
        _ => Err(RuntimeError::type_error(
            "derived constructor returned a non-object",
        )),
    }
}

pub(super) fn ordinary_this(realm: &Realm, value: Value) -> RuntimeResult<Value> {
    match value {
        Value::Undefined | Value::Null => realm.global_this_value(),
        Value::Object(_) => Ok(value),
        primitive => box_primitive(realm, primitive).map(Value::Object),
    }
}

pub(super) fn box_primitive(realm: &Realm, value: Value) -> RuntimeResult<Object> {
    use super::IntrinsicId;
    let id = match &value {
        Value::Object(object) => return Ok(object.clone()),
        Value::Undefined | Value::Null => {
            return Err(RuntimeError::type_error("cannot box null or undefined"));
        }
        Value::Number(_) => IntrinsicId::NumberPrototype,
        Value::String(_) => IntrinsicId::StringPrototype,
        Value::Boolean(_) => IntrinsicId::BooleanPrototype,
        Value::Symbol(_) => IntrinsicId::SymbolPrototype,
        Value::BigInt(_) => IntrinsicId::BigIntPrototype,
    };
    Ok(super::intrinsics::boxed_value(&realm.intrinsic(id), value))
}

pub(super) fn bind_arguments(
    realm: &Realm,
    environment: &Environment,
    arguments: &[Value],
) -> RuntimeResult<()> {
    let object = Object::new_for(&realm.global_object());
    for (index, value) in arguments.iter().enumerate() {
        object.create_data_property(
            PropertyKey::String(JsString::from(index.to_string())),
            value.clone(),
        )?;
    }
    object.create_data_property("length".into(), Value::Number(arguments.len() as f64))?;
    let name = JsString::from("arguments");
    environment.create_mutable_binding(name.clone(), false)?;
    environment.initialize_binding(&name, Value::Object(object))
}

pub(super) fn bind_parameters(
    evaluator: &ExpressionEvaluator,
    environment: &Environment,
    parameters: &[Expr],
    arguments: &[Value],
) -> RuntimeResult<()> {
    let mut index = 0;
    for parameter in parameters {
        match &parameter.kind {
            ExprKind::Spread(binding) => {
                let rest = Object::new_for(&evaluator.realm.global_object());
                for (rest_index, value) in arguments[index..].iter().enumerate() {
                    rest.create_data_property(
                        PropertyKey::String(JsString::from(rest_index.to_string())),
                        value.clone(),
                    )?;
                }
                rest.create_data_property(
                    "length".into(),
                    Value::Number(arguments.len().saturating_sub(index) as f64),
                )?;
                bind_parameter(environment, binding, Value::Object(rest))?;
                index = arguments.len();
            }
            ExprKind::Assignment { target, value, .. } => {
                let argument = arguments.get(index).cloned().unwrap_or(Value::Undefined);
                let argument = if matches!(argument, Value::Undefined) {
                    evaluator.evaluate(value)?
                } else {
                    argument
                };
                bind_parameter(environment, target, argument)?;
                index += 1;
            }
            _ => {
                bind_parameter(
                    environment,
                    parameter,
                    arguments.get(index).cloned().unwrap_or(Value::Undefined),
                )?;
                index += 1;
            }
        }
    }
    Ok(())
}

fn bind_parameter(environment: &Environment, binding: &Expr, value: Value) -> RuntimeResult<()> {
    let ExprKind::Identifier(name) = &binding.kind else {
        return Err(RuntimeError::internal_error(
            "destructuring parameter binding is not implemented",
        ));
    };
    let name = JsString::from(name.as_str());
    if environment.has_binding(&name)? {
        environment.set_mutable_binding(&name, value, false)
    } else {
        environment.create_mutable_binding(name.clone(), false)?;
        environment.initialize_binding(&name, value)
    }
}

#[cfg(test)]
mod primitive_this_tests {
    use super::*;
    use crate::{
        bytecode::{BytecodeCompiler, VirtualMachine},
        runtime::{Runtime, ScriptRecord},
    };

    #[test]
    fn ast_and_vm_use_the_realms_custom_global_this_value() {
        for bytecode in [false, true] {
            let runtime = Runtime::new();
            runtime.set_allocation_collection_interval(1);
            let global = Object::new_in(&runtime);
            let receiver = Object::new_in(&runtime);
            receiver
                .create_data_property("answer".into(), Value::Number(42.0))
                .unwrap();
            let realm = Realm::try_new(global.clone(), Value::Object(receiver.clone())).unwrap();
            drop(receiver);
            runtime.collect_garbage().unwrap();
            let source = "function read(){return this.answer;}read();";
            let completion = if bytecode {
                VirtualMachine::execute_in_realm(
                    &BytecodeCompiler::compile(source).unwrap(),
                    &realm,
                )
                .unwrap()
            } else {
                ScriptRecord::parse(realm.clone(), source)
                    .unwrap()
                    .evaluate()
                    .unwrap()
            };
            assert!(
                matches!(completion, Completion::Normal(Some(Value::Number(value))) if value == 42.0)
            );
            drop((global, realm));
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        }
    }
}

#[cfg(test)]
mod auto_body_tests {
    use crate::{Engine, embedding::ScriptBackend, runtime::Value};

    #[test]
    fn only_auto_fallback_enters_the_verified_function_runtime() {
        for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
            let engine = Engine::builder().script_backend(backend).build();
            let realm = engine.create_realm();
            let script = engine.compile(&realm, "class C { static total(n) { let sum = 0; for (let i = 0; i < n; i++) { sum += i; } return sum; } }").unwrap();
            assert_eq!(script.backend(), ScriptBackend::Ast);
            engine.evaluate(&script).unwrap();
            let call = engine.compile(&realm, "C.total(4);").unwrap();
            let before = super::super::bytecode_function::bridged_calls();
            assert!(
                engine
                    .evaluate(&call)
                    .unwrap()
                    .value()
                    .same_value(&Value::Number(6.0))
            );
            assert_eq!(
                super::super::bytecode_function::bridged_calls() - before,
                usize::from(backend == ScriptBackend::Auto)
            );
        }
    }
}

#[cfg(test)]
mod derived_arguments_tests {
    use crate::{
        Engine,
        embedding::ScriptBackend,
        runtime::{Object, Value},
    };

    #[test]
    fn anonymous_class_definitions_avoid_an_unused_environment() {
        for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
            let engine = Engine::builder().script_backend(backend).build();
            engine
                .allocation_context()
                .set_allocation_collection_interval(usize::MAX);
            {
                let realm = engine.create_realm();
                let anonymous = engine
                    .compile(&realm, "(class { read(){return 42;} });")
                    .unwrap();
                let named = engine
                    .compile(&realm, "(class Named { read(){return 42;} });")
                    .unwrap();
                let count = if cfg!(miri) { 2 } else { 8 };
                let allocations = |script| {
                    let before = engine.allocation_context().diagnostics();
                    for _ in 0..count {
                        engine.evaluate(script).unwrap();
                    }
                    let after = engine.allocation_context().diagnostics();
                    assert_eq!(after.collections, before.collections);
                    after.live - before.live
                };
                assert_eq!(
                    allocations(&named) - allocations(&anonymous),
                    count,
                    "{backend:?}"
                );
            }
            assert_eq!(engine.collect_garbage().unwrap().retained, 0);
        }
    }

    #[test]
    fn derived_arguments_and_delegation_each_avoid_one_managed_object_per_call() {
        for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
            let engine = Engine::builder().script_backend(backend).build();
            engine
                .allocation_context()
                .set_allocation_collection_interval(usize::MAX);
            {
                let realm = engine.create_realm();
                let definition = engine.compile(&realm, "class A {} class Delegate extends A { constructor(value) { super(value); } } class Lean extends A { constructor(value) { super(value); this.marker = value; } } class Observed extends A { constructor(value) { arguments.length; super(value); this.marker = value; } }").unwrap();
                engine.evaluate(&definition).unwrap();
                let get = |name: &str| {
                    let script = engine.compile(&realm, name).unwrap();
                    let value = engine.evaluate(&script).unwrap();
                    let Value::Object(object) = value.value() else {
                        panic!("constructor")
                    };
                    object.clone()
                };
                let (lean, observed) = (get("Lean"), get("Observed"));
                let count = if cfg!(miri) { 2 } else { 8 };
                let allocations = |constructor: &Object| {
                    let before = engine.allocation_context().diagnostics();
                    for _ in 0..count {
                        constructor
                            .construct(&[Value::Number(42.0)], constructor.clone())
                            .unwrap();
                    }
                    let after = engine.allocation_context().diagnostics();
                    assert_eq!(after.collections, before.collections);
                    after.live - before.live
                };
                let delegated_allocations = allocations(&get("Delegate"));
                let lean_allocations = allocations(&lean);
                assert_eq!(
                    lean_allocations - delegated_allocations,
                    count,
                    "{backend:?}: activation"
                );
                let observed_allocations = allocations(&observed);
                assert_eq!(
                    observed_allocations - lean_allocations,
                    count,
                    "{backend:?}"
                );
            }
            assert_eq!(engine.collect_garbage().unwrap().retained, 0);
        }
    }

    #[test]
    fn complete_syntax_keeps_arguments_for_eval_and_nested_observers() {
        for source in [
            "function f() { eval('arguments'); }",
            "function f() { return () => arguments; }",
            "function f(value = arguments.length) {}",
            "function f() { try {} finally { arguments; } }",
            "function f() { class C { static read() { return arguments; } } }",
        ] {
            let expression = crate::parser::parse_expression(source).unwrap();
            let crate::ast::ExprKind::Function(function) = expression.kind else {
                panic!("function")
            };
            assert!(super::may_observe_arguments(&function), "{source}");
        }
    }
}
