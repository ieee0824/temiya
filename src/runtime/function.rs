//! ECMAScript function objects, calls, constructors, and parameter binding.

use super::{
    environment::StoredEnvironment,
    execution::StoredRealm,
    stored::{StoredObject, StoredValue},
};
use std::rc::Rc;

use crate::ast::{Expr, ExprKind, Function, FunctionBody, MethodKind};

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

#[derive(Clone)]
pub(super) struct FunctionClosure {
    pub(super) code: Rc<Function>,
    pub(super) kind: FunctionKind,
    pub(super) realm: Realm,
    pub(super) environment: Environment,
    pub(super) lexical_this: Value,
    #[allow(dead_code)]
    home_object: Option<Object>,
}

pub(super) struct StoredFunctionClosure {
    code: Rc<Function>,
    kind: FunctionKind,
    realm: StoredRealm,
    environment: StoredEnvironment,
    lexical_this: StoredValue,
    home_object: Option<StoredObject>,
}
impl StoredFunctionClosure {
    pub(super) fn has_legacy_properties(&self) -> bool {
        self.kind == FunctionKind::Normal
            && !self.code.strict
            && !self.code.generator
            && !self.code.asynchronous
    }
    pub(super) fn source_realm(&self) -> &StoredRealm {
        &self.realm
    }
    fn load(&self, owner: &Object) -> RuntimeResult<FunctionClosure> {
        Ok(FunctionClosure {
            code: self.code.clone(),
            kind: self.kind,
            realm: self.realm.load(owner)?,
            environment: self.environment.load(owner)?,
            lexical_this: self.lexical_this.load(owner)?,
            home_object: self
                .home_object
                .as_ref()
                .map(|object| object.load(owner))
                .transpose()?,
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
        let closure = self.load(owner)?;
        construct_function(&closure, owner.clone(), arguments, new_target)
            .map_err(|error| closure.realm.bind_exception(error))
    }
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        self.realm.trace(tracer);
        self.environment.trace(tracer);
        self.lexical_this.trace(tracer);
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
    let function = Object::new_with_prototype(
        evaluator
            .realm
            .intrinsic(super::IntrinsicId::FunctionPrototype),
    );
    let closure = StoredFunctionClosure {
        code: Rc::new(code.clone()),
        kind,
        realm: StoredRealm::store(&function, &evaluator.realm)?,
        environment: StoredEnvironment::store(
            &function,
            name_environment.as_ref().unwrap_or(&evaluator.environment),
        )?,
        lexical_this: StoredValue::store(&function, &evaluator.this_value)?,
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
    function.define_own_property(
        "length".into(),
        super::PropertyDescriptor::data(Value::Number(length as f64), false, false, true),
    )?;
    if let Some(ExprKind::Identifier(name)) = code.name.as_ref().map(|name| &name.kind) {
        function
            .create_data_property("name".into(), Value::String(JsString::from(name.as_str())))?;
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
    if !lexical {
        bind_arguments(&closure.realm, &environment, arguments)?;
    }
    let evaluator = ExpressionEvaluator::with_environment(
        closure.realm.clone(),
        environment.clone(),
        this_value.clone(),
        closure.code.strict,
    )
    .with_new_target(new_target.clone());
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
            .with_new_target(new_target);
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
) -> RuntimeResult<Object> {
    let prototype = match new_target.get(&PropertyKey::from("prototype"))? {
        Value::Object(prototype) => Some(prototype),
        _ => Some(closure.realm.intrinsic(super::IntrinsicId::ObjectPrototype)),
    };
    let instance = Object::new_for(&new_target);
    if let Some(prototype) = prototype {
        instance.initialize_prototype(Some(prototype));
    }
    let result = call_function(
        closure,
        function,
        Value::Object(instance.clone()),
        arguments,
        Some(new_target),
    )?;
    match result {
        Value::Object(object) => Ok(object),
        _ => Ok(instance),
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
