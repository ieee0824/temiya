//! Fundamental Object, Function, and Reflect built-ins.

use std::{cell::RefCell, rc::Rc};

use super::{
    IntrinsicId, IntrinsicRegistry, JsString, Object, PropertyDescriptor, PropertyKey,
    RuntimeError, RuntimeResult, Value,
};

pub(super) fn initialize(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    install_object(intrinsics)?;
    install_function(intrinsics)?;
    install_reflect(intrinsics)
}

fn install_object(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let constructor = intrinsics.get(IntrinsicId::Object);
    let object_prototype = intrinsics.get(IntrinsicId::ObjectPrototype);
    method(&constructor, "create", move |_, arguments| {
        let prototype = nullable_object(arguments.first())?;
        let object = Object::new();
        object.set_prototype(prototype);
        match arguments.get(1) {
            None | Some(Value::Undefined) => {}
            Some(Value::Object(descriptors)) => define_properties(&object, descriptors)?,
            Some(_) => {
                return Err(RuntimeError::type_error(
                    "Object.create descriptors must be an object",
                ));
            }
        }
        Ok(Value::Object(object))
    })?;
    method(&constructor, "defineProperty", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.defineProperty target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        let descriptor = descriptor_arg(arguments.get(2))?;
        require_define(&object, key, descriptor)?;
        Ok(Value::Object(object))
    })?;
    method(&constructor, "getOwnPropertyDescriptor", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.getOwnPropertyDescriptor target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        object
            .get_own_property(&key)
            .map_or(Ok(Value::Undefined), |descriptor| {
                descriptor.to_object().map(Value::Object)
            })
    })?;
    method(&constructor, "getPrototypeOf", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.getPrototypeOf target")?;
        Ok(object.prototype().map_or(Value::Null, Value::Object))
    })?;
    method(&constructor, "setPrototypeOf", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.setPrototypeOf target")?;
        let prototype = nullable_object(arguments.get(1))?;
        if !object.set_prototype(prototype) {
            return Err(RuntimeError::type_error(
                "object prototype cannot be changed",
            ));
        }
        Ok(Value::Object(object))
    })?;
    method(&constructor, "preventExtensions", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.preventExtensions target")?;
        object.prevent_extensions();
        Ok(Value::Object(object))
    })?;
    method(&constructor, "isExtensible", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.isExtensible target")?;
        Ok(Value::Boolean(object.is_extensible()))
    })?;
    method(&object_prototype, "hasOwnProperty", |this, arguments| {
        let object = object_value(&this, "Object.prototype.hasOwnProperty receiver")?;
        let key = value_arg(arguments.first()).to_property_key()?;
        Ok(Value::Boolean(object.get_own_property(&key).is_some()))
    })
}

fn install_function(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let prototype = intrinsics.get(IntrinsicId::FunctionPrototype);
    method(&prototype, "call", |this, arguments| {
        let target = callable_value(&this)?;
        target.call(
            arguments.first().cloned().unwrap_or(Value::Undefined),
            arguments.get(1..).unwrap_or_default(),
        )
    })?;
    method(&prototype, "apply", |this, arguments| {
        let target = callable_value(&this)?;
        let this_argument = arguments.first().cloned().unwrap_or(Value::Undefined);
        let list = argument_list(arguments.get(1))?;
        target.call(this_argument, &list)
    })?;
    method(&prototype, "bind", |this, arguments| {
        let target = callable_value(&this)?;
        let bound_this = arguments.first().cloned().unwrap_or(Value::Undefined);
        let prefix = arguments.get(1..).unwrap_or_default().to_vec();
        Ok(Value::Object(bound_function(target, bound_this, prefix)))
    })
}

fn install_reflect(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let reflect = intrinsics.get(IntrinsicId::Reflect);
    method(&reflect, "apply", |_, arguments| {
        let target = callable_arg(arguments.first())?;
        let this_argument = arguments.get(1).cloned().unwrap_or(Value::Undefined);
        target.call(this_argument, &argument_list(arguments.get(2))?)
    })?;
    method(&reflect, "construct", |_, arguments| {
        let target = constructor_arg(arguments.first())?;
        let list = argument_list(arguments.get(1))?;
        let new_target = match arguments.get(2) {
            None => target.clone(),
            Some(value) => constructor_arg(Some(value))?,
        };
        target.construct(&list, new_target).map(Value::Object)
    })?;
    method(&reflect, "defineProperty", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.defineProperty target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        let descriptor = descriptor_arg(arguments.get(2))?;
        object
            .define_own_property(key, descriptor)
            .map(Value::Boolean)
    })?;
    method(&reflect, "getOwnPropertyDescriptor", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.getOwnPropertyDescriptor target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        object
            .get_own_property(&key)
            .map_or(Ok(Value::Undefined), |descriptor| {
                descriptor.to_object().map(Value::Object)
            })
    })?;
    method(&reflect, "get", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.get target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        let receiver = arguments
            .get(2)
            .cloned()
            .unwrap_or_else(|| Value::Object(object.clone()));
        object.get_with_receiver(&key, receiver)
    })?;
    method(&reflect, "set", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.set target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        let value = value_arg(arguments.get(2));
        let receiver = arguments
            .get(3)
            .cloned()
            .unwrap_or_else(|| Value::Object(object.clone()));
        object
            .set_with_receiver(&key, value, receiver)
            .map(Value::Boolean)
    })?;
    method(&reflect, "has", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.has target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        object.has_property(&key).map(Value::Boolean)
    })?;
    method(&reflect, "deleteProperty", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.deleteProperty target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        Ok(Value::Boolean(object.delete(&key)))
    })?;
    method(&reflect, "ownKeys", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.ownKeys target")?;
        let values = object
            .own_property_keys()
            .into_iter()
            .map(|key| match key {
                PropertyKey::String(value) => Value::String(value),
                PropertyKey::Symbol(value) => Value::Symbol(value),
            })
            .collect::<Vec<_>>();
        array_value(values)
    })?;
    method(&reflect, "getPrototypeOf", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.getPrototypeOf target")?;
        Ok(object.prototype().map_or(Value::Null, Value::Object))
    })?;
    method(&reflect, "setPrototypeOf", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.setPrototypeOf target")?;
        let prototype = nullable_object(arguments.get(1))?;
        Ok(Value::Boolean(object.set_prototype(prototype)))
    })?;
    method(&reflect, "isExtensible", |_, arguments| {
        Ok(Value::Boolean(
            object_arg(arguments.first(), "Reflect.isExtensible target")?.is_extensible(),
        ))
    })?;
    method(&reflect, "preventExtensions", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.preventExtensions target")?;
        object.prevent_extensions();
        Ok(Value::Boolean(true))
    })
}

fn method(
    holder: &Object,
    name: &str,
    callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
) -> RuntimeResult<()> {
    require_define(
        holder,
        name.into(),
        PropertyDescriptor::data(
            Value::Object(Object::native_function(callback)),
            true,
            false,
            true,
        ),
    )
}

fn bound_function(target: Object, bound_this: Value, prefix: Vec<Value>) -> Object {
    let call_target = target.clone();
    let call_prefix = prefix.clone();
    let call = move |_: Value, suffix: &[Value]| {
        let mut arguments = call_prefix.clone();
        arguments.extend_from_slice(suffix);
        call_target.call(bound_this.clone(), &arguments)
    };
    if target.is_constructor() {
        let slot = Rc::new(RefCell::new(None::<Object>));
        let construct_slot = slot.clone();
        let construct_target = target.clone();
        let construct_prefix = prefix;
        let function = Object::native_callable_constructor(call, move |suffix, new_target| {
            let mut arguments = construct_prefix.clone();
            arguments.extend_from_slice(suffix);
            let actual_new_target = if construct_slot.borrow().as_ref() == Some(&new_target) {
                construct_target.clone()
            } else {
                new_target
            };
            construct_target.construct(&arguments, actual_new_target)
        });
        *slot.borrow_mut() = Some(function.clone());
        function
    } else {
        Object::native_function(call)
    }
}

fn define_properties(target: &Object, descriptors: &Object) -> RuntimeResult<()> {
    for key in descriptors.own_property_keys() {
        let Value::Object(descriptor) = descriptors.get(&key)? else {
            return Err(RuntimeError::type_error(
                "property descriptor must be an object",
            ));
        };
        require_define(target, key, PropertyDescriptor::from_object(&descriptor)?)?;
    }
    Ok(())
}

fn descriptor_arg(value: Option<&Value>) -> RuntimeResult<PropertyDescriptor> {
    let object = object_arg(value, "property descriptor")?;
    PropertyDescriptor::from_object(&object)
}

fn require_define(
    object: &Object,
    key: PropertyKey,
    descriptor: PropertyDescriptor,
) -> RuntimeResult<()> {
    if object.define_own_property(key, descriptor)? {
        Ok(())
    } else {
        Err(RuntimeError::type_error("property cannot be defined"))
    }
}

fn value_arg(value: Option<&Value>) -> Value {
    value.cloned().unwrap_or(Value::Undefined)
}

fn object_arg(value: Option<&Value>, name: &str) -> RuntimeResult<Object> {
    object_value(&value_arg(value), name)
}

fn object_value(value: &Value, name: &str) -> RuntimeResult<Object> {
    match value {
        Value::Object(object) => Ok(object.clone()),
        _ => Err(RuntimeError::type_error(format!(
            "{name} must be an object"
        ))),
    }
}

fn callable_arg(value: Option<&Value>) -> RuntimeResult<Object> {
    callable_value(&value_arg(value))
}

fn callable_value(value: &Value) -> RuntimeResult<Object> {
    let object = object_value(value, "function target")?;
    if object.is_callable() {
        Ok(object)
    } else {
        Err(RuntimeError::type_error("function target is not callable"))
    }
}

fn constructor_arg(value: Option<&Value>) -> RuntimeResult<Object> {
    let object = object_arg(value, "constructor target")?;
    if object.is_constructor() {
        Ok(object)
    } else {
        Err(RuntimeError::type_error("target is not a constructor"))
    }
}

fn nullable_object(value: Option<&Value>) -> RuntimeResult<Option<Object>> {
    match value_arg(value) {
        Value::Null => Ok(None),
        Value::Object(object) => Ok(Some(object)),
        _ => Err(RuntimeError::type_error(
            "prototype must be an object or null",
        )),
    }
}

fn argument_list(value: Option<&Value>) -> RuntimeResult<Vec<Value>> {
    match value_arg(value) {
        Value::Undefined | Value::Null => Ok(Vec::new()),
        Value::Object(object) => {
            let length = object.get(&"length".into())?.to_number()?;
            if !length.is_finite() || length < 0.0 || length > 100_000.0 {
                return Err(RuntimeError::range_error("argument list length is invalid"));
            }
            (0..length.floor() as u32)
                .map(|index| object.get(&JsString::from(index.to_string()).into()))
                .collect()
        }
        _ => Err(RuntimeError::type_error("argument list must be an object")),
    }
}

fn array_value(values: Vec<Value>) -> RuntimeResult<Value> {
    let array = Object::new();
    let length = values.len();
    for (index, value) in values.into_iter().enumerate() {
        array.create_data_property(JsString::from(index.to_string()).into(), value)?;
    }
    array.define_own_property(
        "length".into(),
        PropertyDescriptor::data(Value::Number(length as f64), true, false, false),
    )?;
    Ok(Value::Object(array))
}
