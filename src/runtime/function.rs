//! ECMAScript function objects, calls, constructors, and parameter binding.

mod constructor_assignments;
mod private_expression;
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
type PreparedFunctionSite = ((usize, u8), Rc<FunctionCode>);
#[derive(Debug, Default)]
pub(super) struct CodeCache {
    entries: HashMap<FunctionCacheKey, Vec<Rc<FunctionCode>>>,
    // Only nodes in these owned, immutable syntax trees enter the identity
    // table. Keeping the owners alive prevents address reuse for cached sites.
    definitions: HashMap<usize, Rc<Expr>>,
    sites: HashMap<(usize, u8), Option<Rc<FunctionCode>>>,
    // Only pinned sites enter this two-entry lookup cache; no borrowed syntax
    // address or JS activation is retained here.
    recent_sites: [Option<PreparedFunctionSite>; 2],
    recent_cursor: usize,
    last_definition: Option<usize>,
}
pub(super) type FunctionCache = Rc<RefCell<CodeCache>>;

impl CodeCache {
    fn remember_prepared_site(&mut self, site: (usize, u8), prepared: &Rc<FunctionCode>) {
        if let Some(slot) = self.sites.get_mut(&site) {
            *slot = Some(prepared.clone());
            self.recent_sites[self.recent_cursor] = Some((site, prepared.clone()));
            self.recent_cursor ^= 1;
        }
    }
    fn retain_site(&mut self, expression: &Expr) {
        let (function, kind) = match &expression.kind {
            ExprKind::Function(function) => (function.as_ref(), FunctionKind::Normal),
            ExprKind::Arrow(function) => (function.as_ref(), FunctionKind::Arrow),
            ExprKind::Method { function, kind } => (function.as_ref(), FunctionKind::Method(*kind)),
            _ => return,
        };
        self.sites
            .entry((std::ptr::from_ref(function) as usize, kind_tag(kind)))
            .or_default();
    }
}

fn kind_tag(kind: FunctionKind) -> u8 {
    match kind {
        FunctionKind::Normal => 0,
        FunctionKind::Arrow => 1,
        FunctionKind::Method(MethodKind::Method) => 2,
        FunctionKind::Method(MethodKind::Get) => 3,
        FunctionKind::Method(MethodKind::Set) => 4,
        FunctionKind::Method(MethodKind::Constructor) => 5,
    }
}

pub(super) fn retain_definition(cache: &FunctionCache, definition: &Rc<Expr>) {
    let mut cache = cache.borrow_mut();
    let identity = Rc::as_ptr(definition) as usize;
    if cache.last_definition == Some(identity) {
        return;
    }
    cache.last_definition = Some(identity);
    if cache.definitions.contains_key(&identity) {
        return;
    }
    definition.visit(&mut |expression| cache.retain_site(expression));
    cache.definitions.insert(identity, definition.clone());
}

#[derive(Debug)]
pub(super) struct FunctionCode {
    ast: Function,
    kind: FunctionKind,
    compiled: Option<Rc<FunctionTemplate>>,
    compiled_body: Option<crate::bytecode::VerifiedBytecode>,
    lexical_property: Option<PropertyKey>,
    private_expression: Option<private_expression::Plan>,
    constructor_assignments: Option<constructor_assignments::Plan>,
    may_observe_arguments: bool,
    parameter_binds_arguments: bool,
    parameter_names: Option<Box<[JsString]>>,
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
    let tag = kind_tag(kind);
    let key = (code.span.start, code.span.end, tag);
    let site = (std::ptr::from_ref(code) as usize, tag);
    if let Some(cache) = cache {
        let found = {
            let cache = cache.borrow();
            if let Some((_, found)) = cache
                .recent_sites
                .iter()
                .flatten()
                .find(|(key, _)| *key == site)
            {
                return found.clone();
            }
            cache.sites.get(&site).and_then(Clone::clone).or_else(|| {
                cache
                    .entries
                    .get(&key)
                    .and_then(|entries| {
                        entries
                            .iter()
                            .find(|entry| entry.kind == kind && entry.ast == *code)
                    })
                    .cloned()
            })
        };
        if let Some(found) = found {
            cache.borrow_mut().remember_prepared_site(site, &found);
            return found;
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
        compiled_body: if cache.is_some() && compiled.is_none() && kind != FunctionKind::Arrow {
            BytecodeCompiler::compile_prepared_function_body(code).ok()
        } else {
            None
        },
        compiled,
        private_expression: cache
            .as_ref()
            .and_then(|_| private_expression::prepare(code, kind)),
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
        parameter_binds_arguments: code.parameters.iter().any(|parameter| {
            crate::semantics::bound_names(parameter)
                .iter()
                .any(|binding| binding.name == "arguments")
        }),
        // Syntax-only preparation: collect all names before any call can run
        // user code. Unsupported patterns retain the call-time error path.
        parameter_names: if code
            .parameters
            .iter()
            .any(|p| !matches!(p.kind, ExprKind::Identifier(_)))
        {
            let mut names = Vec::new();
            code.parameters
                .iter()
                .try_for_each(|p| super::binding::names(p, &mut names))
                .ok()
                .map(|()| names.into_boxed_slice())
        } else {
            None
        },
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
        let mut cache = cache.borrow_mut();
        cache.entries.entry(key).or_default().push(prepared.clone());
        // entries owns this FunctionCode and its cloned AST. Nested syntax
        // evaluated by the AST interpreter is therefore stable as well.
        prepared
            .ast
            .visit_expressions(&mut |expression| cache.retain_site(expression));
        cache.remember_prepared_site(site, &prepared);
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

// Bound private identities contain no JS edges and never enter shared code.
// One optional box preserves the size of ordinary closure payloads.
enum PrivateBindings {
    Expression(super::private_fields::Name),
    Constructor(constructor_assignments::Names),
}

pub(super) struct StoredFunctionClosure {
    code: Rc<FunctionCode>,
    private_bindings: Option<Box<PrivateBindings>>,
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
            // Only arrows observe the definition's this. Ordinary functions and
            // methods receive their own this binding, including AST fallbacks.
            lexical_this: if self.kind == FunctionKind::Arrow {
                self.lexical_this.load(owner)?
            } else {
                Value::Undefined
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
        if let Some(plan) = &self.code.private_expression {
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
        if self.constructor_kind == ConstructorKind::Base {
            if let Some(plan) = &self.code.constructor_assignments {
                // This prepared body cannot observe an activation or lexical
                // this. Do not revive those unused roots for every instance.
                let realm = self.realm.load(owner)?;
                // Prototype/allocation failures occur before entering the body
                // and retain the caller realm; bind only body errors below.
                let prototype = match prototype {
                    Some(prototype) => prototype,
                    None => new_target
                        .constructor_prototype(super::IntrinsicId::ObjectPrototype, || {
                            Ok(realm.intrinsic(super::IntrinsicId::ObjectPrototype))
                        })?,
                };
                let instance = Object::new_for(&new_target);
                instance.initialize_prototype(Some(prototype));
                owner.initialize_instance_fields(&instance)?;
                let bindings = match self.private_bindings.as_deref() {
                    Some(PrivateBindings::Constructor(names)) => Some(names),
                    _ => None,
                };
                plan.execute(&instance, arguments, bindings, || {
                    self.environment.load(owner)
                })
                .map_err(|error| realm.bind_exception(error))?;
                return Ok(instance);
            }
        }
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
    let prepared = prepare_code(code, kind, &evaluator.function_cache);
    let captured = name_environment.as_ref().unwrap_or(&evaluator.environment);
    let private_bindings = if let Some(plan) = &prepared.private_expression {
        plan.bind(captured).map(PrivateBindings::Expression)
    } else if constructor_kind == ConstructorKind::Base {
        prepared
            .constructor_assignments
            .as_ref()
            .and_then(|plan| plan.bind(captured).map(PrivateBindings::Constructor))
    } else {
        None
    }
    .map(Box::new);
    let closure = StoredFunctionClosure {
        code: prepared,
        private_bindings,
        kind,
        realm: StoredRealm::store(&function, &evaluator.realm)?,
        environment: StoredEnvironment::store(
            &function,
            name_environment.as_ref().unwrap_or(&evaluator.environment),
        )?,
        lexical_this: if kind == FunctionKind::Arrow {
            StoredValue::store(&function, &evaluator.this_value)?
        } else {
            StoredValue::Undefined
        },
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
        function.initialize_function_length(length, constructable);
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
    let arguments_needed =
        !lexical && closure.code.may_observe_arguments && !closure.code.parameter_binds_arguments;
    let status = if lexical {
        ThisBindingStatus::Lexical
    } else {
        ThisBindingStatus::Uninitialized
    };
    let environment = if let Some(names) = closure.code.parameter_names.as_deref() {
        Environment::function_with_uninitialized_parameters(
            Some(closure.environment.clone()),
            function,
            new_target.clone(),
            status,
            names,
            usize::from(arguments_needed),
        )
    } else {
        Environment::function(
            Some(closure.environment.clone()),
            function,
            new_target.clone(),
            status,
        )
    };
    if !lexical {
        environment.bind_this_value(this_value.clone())?;
    }
    // The complete syntax scan includes defaults, nested classes/functions,
    // arrows and direct eval. Retain the actual object whenever any of those
    // can observe arguments; otherwise this implicit binding is unreachable.
    if arguments_needed {
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
    bind_parameters_impl(
        &evaluator,
        &environment,
        &closure.code.parameters,
        closure.code.parameter_names.as_deref(),
        arguments,
        closure.code.parameter_names.is_some(),
    )?;

    if let Some(body) = &closure.code.compiled_body {
        return match crate::bytecode::VirtualMachine::execute_in_function(
            body,
            &closure.realm,
            &environment,
        )? {
            Completion::Return(value) => Ok(value),
            Completion::Normal(_) => Ok(Value::Undefined),
            Completion::Throw(value) => Err(RuntimeError::thrown(value)),
            _ => Err(RuntimeError::internal_error(
                "invalid bytecode function completion",
            )),
        };
    }

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
    if closure.code.may_observe_arguments && !closure.code.parameter_binds_arguments {
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
        closure.code.parameter_names.as_deref(),
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
    let object = Object::indexed_values_object(&realm.global_object(), arguments)?;
    let name = JsString::from("arguments");
    environment.create_mutable_binding(name.clone(), false)?;
    environment.initialize_binding(&name, Value::Object(object))
}

pub(super) fn bind_parameters(
    evaluator: &ExpressionEvaluator,
    environment: &Environment,
    parameters: &[Expr],
    prepared_names: Option<&[JsString]>,
    arguments: &[Value],
) -> RuntimeResult<()> {
    bind_parameters_impl(
        evaluator,
        environment,
        parameters,
        prepared_names,
        arguments,
        false,
    )
}

fn bind_parameters_impl(
    evaluator: &ExpressionEvaluator,
    environment: &Environment,
    parameters: &[Expr],
    prepared_names: Option<&[JsString]>,
    arguments: &[Value],
    bindings_prepared: bool,
) -> RuntimeResult<()> {
    if bindings_prepared
        || parameters
            .iter()
            .any(|parameter| !matches!(parameter.kind, ExprKind::Identifier(_)))
    {
        if !bindings_prepared {
            let names = if let Some(names) = prepared_names {
                Cow::Borrowed(names)
            } else {
                let mut names = Vec::new();
                for parameter in parameters {
                    super::binding::names(parameter, &mut names)?;
                }
                Cow::Owned(names)
            };
            for name in names.iter() {
                if !environment.has_binding(name)? {
                    environment.create_mutable_binding(name.clone(), false)?;
                }
            }
        }
        for (index, parameter) in parameters.iter().enumerate() {
            if let ExprKind::Spread(binding) = &parameter.kind {
                let array = super::array_literal::create(&evaluator.realm)?;
                let mut next = 0;
                for value in arguments.get(index..).unwrap_or_default() {
                    super::array_literal::append(&array, &mut next, value.clone())?;
                }
                super::array_literal::finish(&array, next)?;
                super::binding::initialize(evaluator, binding, Value::Object(array), true)?;
                break;
            }
            super::binding::initialize(
                evaluator,
                parameter,
                arguments.get(index).cloned().unwrap_or(Value::Undefined),
                true,
            )?;
        }
        return Ok(());
    }
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
mod owned_definition_tests {
    use super::*;

    fn definition(value: u8) -> Rc<Expr> {
        Rc::new(
            crate::parser::parse_expression(&format!("{{get value(){{return {value};}}}}"))
                .unwrap(),
        )
    }
    fn getter(expression: &Expr) -> &Function {
        let ExprKind::Object(properties) = &expression.kind else {
            panic!("object")
        };
        let ExprKind::Method { function, .. } = &properties[0].value.as_ref().unwrap().kind else {
            panic!("getter")
        };
        function
    }

    #[test]
    fn owned_sites_pin_syntax_and_keep_colliding_spans_distinct() {
        let cache: FunctionCache = Default::default();
        let first = definition(1);
        let second = definition(2);
        assert_eq!(getter(&first).span, getter(&second).span);
        retain_definition(&cache, &first);
        retain_definition(&cache, &second);
        let kind = FunctionKind::Method(MethodKind::Get);
        let a = prepare_code(getter(&first), kind, &Some(cache.clone()));
        let b = prepare_code(getter(&second), kind, &Some(cache.clone()));
        assert!(!Rc::ptr_eq(&a, &b));
        assert!(Rc::ptr_eq(
            &a,
            &prepare_code(getter(&first), kind, &Some(cache.clone()))
        ));
        let weak = Rc::downgrade(&first);
        drop(first);
        assert!(weak.upgrade().is_some());
        assert_eq!(cache.borrow().definitions.len(), 2);
        assert_eq!(cache.borrow().sites.len(), 2);
        drop(cache);
        assert!(
            weak.upgrade().is_none(),
            "syntax owners must release with cache"
        );
    }

    #[test]
    fn hot_sites_evict_without_memoizing_borrowed_syntax_or_owning_cache() {
        let cache: FunctionCache = Default::default();
        let kind = FunctionKind::Method(MethodKind::Get);
        let roots = [definition(1), definition(2), definition(3)];
        let prepared: Vec<_> = roots
            .iter()
            .map(|root| {
                retain_definition(&cache, root);
                retain_definition(&cache, root);
                prepare_code(getter(root), kind, &Some(cache.clone()))
            })
            .collect();
        for _ in 0..3 {
            for (root, expected) in roots.iter().zip(&prepared) {
                assert!(Rc::ptr_eq(
                    expected,
                    &prepare_code(getter(root), kind, &Some(cache.clone()))
                ));
            }
        }
        let borrowed = getter(&roots[0]).clone();
        prepare_code(&borrowed, kind, &Some(cache.clone()));
        assert!(
            cache
                .borrow()
                .recent_sites
                .iter()
                .flatten()
                .all(|(site, _)| site.0 != std::ptr::from_ref(&borrowed) as usize)
        );
        let weak = Rc::downgrade(&cache);
        drop(cache);
        assert!(
            weak.upgrade().is_none(),
            "hot code entries must not retain their cache"
        );
    }

    #[test]
    fn borrowed_clones_and_different_kinds_keep_structural_fallback() {
        let cache: FunctionCache = Default::default();
        let root = definition(1);
        retain_definition(&cache, &root);
        let kind = FunctionKind::Method(MethodKind::Get);
        let a = prepare_code(getter(&root), kind, &Some(cache.clone()));
        let mut borrowed = getter(&root).clone();
        assert!(Rc::ptr_eq(
            &a,
            &prepare_code(&borrowed, kind, &Some(cache.clone()))
        ));
        borrowed.body = getter(&definition(2)).body.clone();
        let b = prepare_code(&borrowed, kind, &Some(cache.clone()));
        assert!(!Rc::ptr_eq(&a, &b));
        let normal = prepare_code(getter(&root), FunctionKind::Normal, &Some(cache.clone()));
        assert!(!Rc::ptr_eq(&normal, &a));
        assert_eq!(
            cache.borrow().sites.len(),
            1,
            "unowned addresses must never be memoized"
        );
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

#[cfg(test)]
mod prepared_body_tests {
    #[test]
    fn non_simple_parameters_prepare_verified_body_without_full_template() {
        let expression =
            crate::parser::parse_expression("function f({a},read=()=>a){a=42;return read();}")
                .unwrap();
        let crate::ast::ExprKind::Function(function) = expression.kind else {
            panic!("function")
        };
        let code = super::prepare_code(
            &function,
            super::FunctionKind::Normal,
            &Some(Default::default()),
        );
        assert!(code.compiled.is_none());
        assert!(code.compiled_body.is_some());
    }
}
