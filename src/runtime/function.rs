//! ECMAScript function objects, calls, constructors, and parameter binding.

use std::{cell::RefCell, rc::Rc};

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
    pub(super) code: Function,
    pub(super) kind: FunctionKind,
    pub(super) realm: Realm,
    pub(super) environment: Environment,
    pub(super) lexical_this: Value,
    #[allow(dead_code)]
    home_object: Option<Object>,
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
            let environment = Environment::declarative(Some(evaluator.environment.clone()));
            environment.create_immutable_binding(JsString::from(name.as_str()), false)?;
            Some(environment)
        } else {
            None
        };
    let closure = Rc::new(FunctionClosure {
        code: code.clone(),
        kind,
        realm: evaluator.realm.clone(),
        environment: name_environment
            .clone()
            .unwrap_or_else(|| evaluator.environment.clone()),
        lexical_this: evaluator.this_value.clone(),
        home_object,
    });
    let slot = Rc::new(RefCell::new(None::<Object>));
    let call_closure = closure.clone();
    let call_slot = slot.clone();
    let call = move |this: Value, arguments: &[Value]| {
        let function = call_slot
            .borrow()
            .clone()
            .ok_or_else(|| RuntimeError::internal_error("function object is not initialized"))?;
        if call_closure.code.generator {
            create_generator_object(&call_closure, function, this, arguments).map(Value::Object)
        } else if call_closure.code.asynchronous {
            settled_promise_object(
                call_closure.realm.job_queue(),
                call_function(&call_closure, function, this, arguments, None)
                    .map_err(|error| call_closure.realm.bind_exception(error)),
            )
            .map(Value::Object)
        } else {
            call_function(&call_closure, function, this, arguments, None)
                .map_err(|error| call_closure.realm.bind_exception(error))
        }
    };
    let constructable = !code.generator
        && !code.asynchronous
        && matches!(
            kind,
            FunctionKind::Normal | FunctionKind::Method(MethodKind::Constructor)
        );
    let function = if constructable {
        let construct_closure = closure.clone();
        let construct_slot = slot.clone();
        Object::native_callable_constructor(call, move |arguments, new_target| {
            let function = construct_slot.borrow().clone().ok_or_else(|| {
                RuntimeError::internal_error("function object is not initialized")
            })?;
            construct_function(&construct_closure, function, arguments, new_target)
                .map_err(|error| construct_closure.realm.bind_exception(error))
        })
    } else {
        Object::native_function(call)
    };
    *slot.borrow_mut() = Some(function.clone());
    if let (Some(environment), Some(ExprKind::Identifier(name))) =
        (name_environment, code.name.as_ref().map(|name| &name.kind))
    {
        environment.initialize_binding(
            &JsString::from(name.as_str()),
            Value::Object(function.clone()),
        )?;
    }

    if constructable {
        let prototype = Object::new();
        prototype.create_data_property("constructor".into(), Value::Object(function.clone()))?;
        function.create_data_property("prototype".into(), Value::Object(prototype))?;
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
    function.create_data_property("length".into(), Value::Number(length as f64))?;
    if let Some(ExprKind::Identifier(name)) = code.name.as_ref().map(|name| &name.kind) {
        function
            .create_data_property("name".into(), Value::String(JsString::from(name.as_str())))?;
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
        bind_arguments(&environment, arguments)?;
    }
    let evaluator = ExpressionEvaluator::with_environment(
        closure.realm.clone(),
        environment.clone(),
        this_value.clone(),
        closure.code.strict,
    )
    .with_new_target(new_target);
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
            );
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
        _ => None,
    };
    let instance = Object::new();
    if let Some(prototype) = prototype {
        instance.set_prototype(Some(prototype));
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
        Value::Undefined | Value::Null => Ok(Value::Object(realm.global_object())),
        Value::Object(_) => Ok(value),
        primitive => {
            let object = Object::new();
            object.create_data_property("[[PrimitiveValue]]".into(), primitive)?;
            Ok(Value::Object(object))
        }
    }
}

pub(super) fn bind_arguments(environment: &Environment, arguments: &[Value]) -> RuntimeResult<()> {
    let object = Object::new();
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
                let rest = Object::new();
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
