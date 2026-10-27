//! Bytecode-backed function objects. No AST body evaluation occurs here.
use std::{cell::RefCell, rc::Rc};

use crate::bytecode::{Constant, FunctionTemplate, Instruction, VirtualMachine};

use super::{
    Completion, Environment, IntrinsicId, Object, PropertyDescriptor, Realm, RuntimeError,
    RuntimeResult, ThisBindingStatus, Value, WeakObject,
};

struct Closure {
    template: Rc<FunctionTemplate>,
    realm: Realm,
    environment: Environment,
}

pub(crate) fn create(
    template: Rc<FunctionTemplate>,
    realm: &Realm,
    environment: Environment,
) -> RuntimeResult<Object> {
    let named = template
        .name
        .as_ref()
        .filter(|_| template.self_binding)
        .map(|name| {
            let scope = Environment::declarative(Some(environment.clone()));
            scope.create_immutable_binding(name.clone(), false)?;
            Ok::<_, RuntimeError>(scope)
        })
        .transpose()?;
    let closure = Rc::new(Closure {
        environment: named.clone().unwrap_or(environment),
        template,
        realm: realm.clone(),
    });
    let slot = Rc::new(RefCell::new(None::<WeakObject>));
    let call_closure = closure.clone();
    let call_slot = slot.clone();
    let callback = move |this, args: &[Value]| {
        let function = call_slot
            .borrow()
            .as_ref()
            .and_then(WeakObject::upgrade)
            .ok_or_else(|| RuntimeError::internal_error("bytecode function not initialized"))?;
        call_closure
            .call(function, this, args, None)
            .map_err(|error| call_closure.realm.bind_exception(error))
    };
    let function = if closure.template.arrow {
        Object::native_function(callback)
    } else {
        let construct_closure = closure.clone();
        let construct_slot = slot.clone();
        Object::native_callable_constructor(callback, move |args, new_target| {
            let function = construct_slot
                .borrow()
                .as_ref()
                .and_then(WeakObject::upgrade)
                .ok_or_else(|| {
                    RuntimeError::internal_error("bytecode constructor not initialized")
                })?;
            let instance = Object::new();
            let prototype = match new_target.get(&"prototype".into())? {
                Value::Object(prototype) => prototype,
                _ => construct_closure
                    .realm
                    .intrinsic(IntrinsicId::ObjectPrototype),
            };
            instance.initialize_prototype(Some(prototype));
            let value = construct_closure
                .call(
                    function,
                    Value::Object(instance.clone()),
                    args,
                    Some(new_target),
                )
                .map_err(|error| construct_closure.realm.bind_exception(error))?;
            Ok(match value {
                Value::Object(object) => object,
                _ => instance,
            })
        })
    };
    *slot.borrow_mut() = Some(function.downgrade());
    function.initialize_prototype(Some(realm.intrinsic(IntrinsicId::FunctionPrototype)));
    for (name, value) in [
        (
            "length",
            Value::Number(closure.template.parameters.len() as f64),
        ),
        (
            "name",
            Value::String(closure.template.name.clone().unwrap_or_default()),
        ),
    ] {
        function.define_own_property(
            name.into(),
            PropertyDescriptor::data(value, false, false, true),
        )?;
    }
    if !closure.template.arrow {
        function.install_lazy_function_prototype(realm.intrinsic(IntrinsicId::ObjectPrototype))?;
    }
    if let Some(named) = named {
        named.initialize_binding(
            closure.template.name.as_ref().expect("named template"),
            Value::Object(function.clone()),
        )?;
    }
    Ok(function)
}

impl Closure {
    fn call(
        &self,
        function: Object,
        this: Value,
        arguments: &[Value],
        new_target: Option<Object>,
    ) -> RuntimeResult<Value> {
        let this_status = if self.template.arrow {
            ThisBindingStatus::Lexical
        } else {
            ThisBindingStatus::Uninitialized
        };
        let frame = if !self.template.arguments_object {
            let body = self.template.body.bytecode();
            let capacity = match body.instructions.first() {
                Some(Instruction::EnterScope(index)) => match &body.constants[*index as usize] {
                    Constant::Scope(scope) => scope.bindings.len(),
                    _ => 0,
                },
                _ => 0,
            };
            Environment::function_with_parameters(
                Some(self.environment.clone()),
                function,
                new_target,
                this_status,
                &self.template.parameters,
                arguments,
                capacity,
            )
        } else {
            Environment::function(
                Some(self.environment.clone()),
                function,
                new_target,
                this_status,
            )
        };
        if !self.template.arrow {
            let this = if self.template.strict {
                this
            } else if matches!(this, Value::Undefined | Value::Null) {
                self.realm.global_environment().global_this_value()?
            } else {
                boxed_this(&self.realm, this)?
            };
            frame.bind_this_value(this)?;
            if self.template.arguments_object {
                super::function::bind_arguments(&frame, arguments)?;
            }
        }
        if self.template.arguments_object {
            for (index, name) in self.template.parameters.iter().enumerate() {
                let argument = arguments.get(index).cloned().unwrap_or(Value::Undefined);
                if frame.has_binding(name)? {
                    frame.set_mutable_binding(name, argument, false)?;
                } else {
                    frame.create_mutable_binding(name.clone(), false)?;
                    frame.initialize_binding(name, argument)?;
                }
            }
        }
        match VirtualMachine::execute_in_function(&self.template.body, &self.realm, &frame)? {
            Completion::Return(value) => Ok(value),
            Completion::Normal(_) => Ok(Value::Undefined),
            Completion::Throw(value) => Err(RuntimeError::thrown(value)),
            _ => Err(RuntimeError::internal_error(
                "invalid bytecode function completion",
            )),
        }
    }
}

fn boxed_this(realm: &Realm, value: Value) -> RuntimeResult<Value> {
    let (object, prototype) = match value {
        Value::Object(_) => return Ok(value),
        Value::String(value) => (
            super::StringObject::new(value).object(),
            IntrinsicId::StringPrototype,
        ),
        Value::Number(value) => (
            super::NumberObject::new(value).object(),
            IntrinsicId::NumberPrototype,
        ),
        Value::BigInt(value) => (
            super::BigIntObject::new(value).object(),
            IntrinsicId::BigIntPrototype,
        ),
        value @ Value::Boolean(_) | value @ Value::Symbol(_) => {
            let prototype = if matches!(value, Value::Boolean(_)) {
                IntrinsicId::BooleanPrototype
            } else {
                IntrinsicId::SymbolPrototype
            };
            let object = Object::new();
            object.0.state.borrow_mut().primitive_data = Some(value);
            (object, prototype)
        }
        Value::Undefined | Value::Null => return realm.global_environment().global_this_value(),
    };
    object.set_prototype(Some(realm.intrinsic(prototype)));
    Ok(Value::Object(object))
}
