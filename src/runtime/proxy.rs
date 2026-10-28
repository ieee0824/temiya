//! Proxy exotic objects and trap invariant enforcement.

use super::{
    ArrayObject, JsString, Object, PropertyDescriptor, PropertyKey, RuntimeError, RuntimeResult,
    Value, is_compatible_property_descriptor,
};

#[derive(Clone)]
pub(super) struct ProxySlot {
    pair: Option<(super::stored::StoredObject, super::stored::StoredObject)>,
    pub(super) callable: bool,
    pub(super) constructor: bool,
}
impl ProxySlot {
    pub(super) fn revoke(&mut self) {
        self.pair = None;
    }
    pub(super) fn target(&self, owner: &Object) -> RuntimeResult<Object> {
        self.pair
            .as_ref()
            .ok_or_else(|| RuntimeError::type_error("revoked Proxy"))?
            .0
            .load(owner)
    }
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        if let Some((target, handler)) = &self.pair {
            target.trace(tracer);
            handler.trace(tracer);
        }
    }
}

/// A revocable Proxy object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxyObject(Object);

impl ProxyObject {
    /// Creates a Proxy around a target and handler object.
    ///
    /// # Panics
    /// Panics if target and handler belong to different allocation contexts.
    /// Use [`Self::try_new`] to handle that error.
    #[must_use]
    pub fn new(target: Object, handler: Object) -> Self {
        Self::try_new(target, handler).expect("Proxy allocation context")
    }

    /// Creates a Proxy, rejecting objects from different allocation contexts.
    pub fn try_new(target: Object, handler: Object) -> RuntimeResult<Self> {
        super::stored::StoredObject::check_context(&target, &handler)?;
        let object = Object::new_for(&target);
        let slot = ProxySlot {
            pair: Some((
                super::stored::StoredObject::store(&object, &target)?,
                super::stored::StoredObject::store(&object, &handler)?,
            )),
            callable: target.is_callable(),
            constructor: target.is_constructor(),
        };
        object.0.state.borrow_mut().proxy = Some(Box::new(slot));
        Ok(Self(object))
    }

    /// Returns the underlying Proxy object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }

    /// Revokes this Proxy. Every later internal operation fails with TypeError.
    pub fn revoke(&self) {
        if let Some(slot) = &mut self.0.0.state.borrow_mut().proxy {
            slot.revoke();
        }
    }
}

fn state(proxy: &Object) -> RuntimeResult<(Object, Object)> {
    let pair = proxy
        .0
        .state
        .borrow()
        .proxy
        .as_ref()
        .expect("Proxy slot")
        .pair
        .clone()
        .ok_or_else(|| RuntimeError::type_error("Proxy has been revoked"))?;
    Ok((pair.0.load(proxy)?, pair.1.load(proxy)?))
}

fn trap(handler: &Object, name: &str) -> RuntimeResult<Option<Object>> {
    match handler.get(&name.into())? {
        Value::Undefined | Value::Null => Ok(None),
        Value::Object(object) if object.is_callable() => Ok(Some(object)),
        _ => Err(RuntimeError::type_error(format!(
            "Proxy {name} trap is not callable"
        ))),
    }
}

fn key_value(key: &PropertyKey) -> Value {
    match key {
        PropertyKey::String(value) => Value::String(value.clone()),
        PropertyKey::Symbol(value) => Value::Symbol(value.clone()),
    }
}

pub(super) fn get(proxy: &Object, key: &PropertyKey, receiver: Value) -> RuntimeResult<Value> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "get")? else {
        return target.get_with_receiver(key, receiver);
    };
    let result = trap.call(
        Value::Object(handler),
        &[Value::Object(target.clone()), key_value(key), receiver],
    )?;
    if let Some(descriptor) = target.try_get_own_property(key)? {
        if descriptor.configurable == Some(false) {
            if descriptor.is_data_descriptor()
                && descriptor.writable == Some(false)
                && !result.same_value(descriptor.value.as_ref().unwrap())
            {
                return Err(invariant("get"));
            }
            if descriptor.is_accessor_descriptor()
                && descriptor.get == Some(None)
                && !matches!(result, Value::Undefined)
            {
                return Err(invariant("get"));
            }
        }
    }
    Ok(result)
}

pub(super) fn set(
    proxy: &Object,
    key: &PropertyKey,
    value: Value,
    receiver: Value,
) -> RuntimeResult<bool> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "set")? else {
        return target.set_with_receiver(key, value, receiver);
    };
    let success = trap
        .call(
            Value::Object(handler),
            &[
                Value::Object(target.clone()),
                key_value(key),
                value.clone(),
                receiver,
            ],
        )?
        .to_boolean();
    if success {
        if let Some(descriptor) = target.try_get_own_property(key)? {
            if descriptor.configurable == Some(false) {
                if descriptor.is_data_descriptor()
                    && descriptor.writable == Some(false)
                    && !value.same_value(descriptor.value.as_ref().unwrap())
                {
                    return Err(invariant("set"));
                }
                if descriptor.is_accessor_descriptor() && descriptor.set == Some(None) {
                    return Err(invariant("set"));
                }
            }
        }
    }
    Ok(success)
}

pub(super) fn has(proxy: &Object, key: &PropertyKey) -> RuntimeResult<bool> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "has")? else {
        return target.has_property(key);
    };
    let result = trap
        .call(
            Value::Object(handler),
            &[Value::Object(target.clone()), key_value(key)],
        )?
        .to_boolean();
    if !result {
        if let Some(descriptor) = target.try_get_own_property(key)? {
            if descriptor.configurable == Some(false) || !target.try_is_extensible()? {
                return Err(invariant("has"));
            }
        }
    }
    Ok(result)
}

pub(super) fn delete(proxy: &Object, key: &PropertyKey) -> RuntimeResult<bool> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "deleteProperty")? else {
        return target.try_delete(key);
    };
    let result = trap
        .call(
            Value::Object(handler),
            &[Value::Object(target.clone()), key_value(key)],
        )?
        .to_boolean();
    if result
        && target
            .try_get_own_property(key)?
            .is_some_and(|descriptor| descriptor.configurable == Some(false))
    {
        return Err(invariant("deleteProperty"));
    }
    Ok(result)
}

pub(super) fn define_own_property(
    proxy: &Object,
    key: PropertyKey,
    descriptor: PropertyDescriptor,
) -> RuntimeResult<bool> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "defineProperty")? else {
        return target.define_own_property(key, descriptor);
    };
    let descriptor_object = descriptor.to_object()?;
    let result = trap
        .call(
            Value::Object(handler),
            &[
                Value::Object(target.clone()),
                key_value(&key),
                Value::Object(descriptor_object),
            ],
        )?
        .to_boolean();
    if result {
        let current = target.try_get_own_property(&key)?;
        if !is_compatible_property_descriptor(
            target.try_is_extensible()?,
            &descriptor,
            current.as_ref(),
        )? {
            return Err(invariant("defineProperty"));
        }
        if descriptor.configurable == Some(false)
            && current
                .as_ref()
                .is_none_or(|value| value.configurable != Some(false))
        {
            return Err(invariant("defineProperty"));
        }
    }
    Ok(result)
}

pub(super) fn get_own_property(
    proxy: &Object,
    key: &PropertyKey,
) -> RuntimeResult<Option<PropertyDescriptor>> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "getOwnPropertyDescriptor")? else {
        return target.try_get_own_property(key);
    };
    let result = trap.call(
        Value::Object(handler),
        &[Value::Object(target.clone()), key_value(key)],
    )?;
    let target_descriptor = target.try_get_own_property(key)?;
    if matches!(result, Value::Undefined) {
        if target_descriptor
            .as_ref()
            .is_some_and(|descriptor| descriptor.configurable == Some(false))
            || (target_descriptor.is_some() && !target.try_is_extensible()?)
        {
            return Err(invariant("getOwnPropertyDescriptor"));
        }
        return Ok(None);
    }
    let Value::Object(result) = result else {
        return Err(RuntimeError::type_error(
            "Proxy descriptor trap must return object or undefined",
        ));
    };
    let descriptor = PropertyDescriptor::from_object(&result)?;
    if !is_compatible_property_descriptor(
        target.try_is_extensible()?,
        &descriptor,
        target_descriptor.as_ref(),
    )? {
        return Err(invariant("getOwnPropertyDescriptor"));
    }
    Ok(Some(descriptor))
}

pub(super) fn own_keys(proxy: &Object) -> RuntimeResult<Vec<PropertyKey>> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "ownKeys")? else {
        return target.try_own_property_keys();
    };
    let Value::Object(result) =
        trap.call(Value::Object(handler), &[Value::Object(target.clone())])?
    else {
        return Err(RuntimeError::type_error(
            "Proxy ownKeys trap must return an object",
        ));
    };
    let length = result.get(&"length".into())?.to_number()?;
    if !length.is_finite() || length < 0.0 {
        return Err(RuntimeError::range_error("invalid ownKeys length"));
    }
    let mut keys = Vec::new();
    for index in 0..length.floor() as u32 {
        let value = result.get(&JsString::from(index.to_string()).into())?;
        let key = match value {
            Value::String(value) => PropertyKey::String(value),
            Value::Symbol(value) => PropertyKey::Symbol(value),
            _ => {
                return Err(RuntimeError::type_error(
                    "Proxy ownKeys values must be strings or symbols",
                ));
            }
        };
        if keys.contains(&key) {
            return Err(invariant("ownKeys duplicate"));
        }
        keys.push(key);
    }
    let target_keys = target.try_own_property_keys()?;
    for key in &target_keys {
        if target
            .try_get_own_property(key)?
            .is_some_and(|descriptor| descriptor.configurable == Some(false))
            && !keys.contains(key)
        {
            return Err(invariant("ownKeys"));
        }
    }
    if !target.try_is_extensible()?
        && (keys.len() != target_keys.len() || target_keys.iter().any(|key| !keys.contains(key)))
    {
        return Err(invariant("ownKeys"));
    }
    Ok(keys)
}

pub(super) fn get_prototype_of(proxy: &Object) -> RuntimeResult<Option<Object>> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "getPrototypeOf")? else {
        return target.try_prototype();
    };
    let result = trap.call(Value::Object(handler), &[Value::Object(target.clone())])?;
    let result = match result {
        Value::Null => None,
        Value::Object(value) => Some(value),
        _ => {
            return Err(RuntimeError::type_error(
                "Proxy prototype trap must return object or null",
            ));
        }
    };
    if !target.try_is_extensible()? && !same_object(&result, &target.try_prototype()?) {
        return Err(invariant("getPrototypeOf"));
    }
    Ok(result)
}

pub(super) fn set_prototype_of(proxy: &Object, prototype: Option<Object>) -> RuntimeResult<bool> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "setPrototypeOf")? else {
        return Ok(target.set_prototype(prototype));
    };
    let value = prototype.clone().map_or(Value::Null, Value::Object);
    let result = trap
        .call(
            Value::Object(handler),
            &[Value::Object(target.clone()), value],
        )?
        .to_boolean();
    if result && !target.try_is_extensible()? && !same_object(&prototype, &target.try_prototype()?)
    {
        return Err(invariant("setPrototypeOf"));
    }
    Ok(result)
}

pub(super) fn is_extensible(proxy: &Object) -> RuntimeResult<bool> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "isExtensible")? else {
        return target.try_is_extensible();
    };
    let result = trap
        .call(Value::Object(handler), &[Value::Object(target.clone())])?
        .to_boolean();
    if result != target.try_is_extensible()? {
        return Err(invariant("isExtensible"));
    }
    Ok(result)
}

pub(super) fn prevent_extensions(proxy: &Object) -> RuntimeResult<bool> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "preventExtensions")? else {
        target.prevent_extensions();
        return Ok(true);
    };
    let result = trap
        .call(Value::Object(handler), &[Value::Object(target.clone())])?
        .to_boolean();
    if result && target.try_is_extensible()? {
        return Err(invariant("preventExtensions"));
    }
    Ok(result)
}

pub(super) fn apply_slot(proxy: &Object, this: Value, arguments: &[Value]) -> RuntimeResult<Value> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "apply")? else {
        return target.call(this, arguments);
    };
    trap.call(
        Value::Object(handler),
        &[
            Value::Object(target),
            this,
            Value::Object(arguments_array(proxy, arguments)?),
        ],
    )
}

pub(super) fn construct_slot(
    proxy: &Object,
    arguments: &[Value],
    new_target: Object,
) -> RuntimeResult<Object> {
    let (target, handler) = state(proxy)?;
    let Some(trap) = trap(&handler, "construct")? else {
        return target.construct(arguments, new_target);
    };
    let result = trap.call(
        Value::Object(handler),
        &[
            Value::Object(target),
            Value::Object(arguments_array(proxy, arguments)?),
            Value::Object(new_target),
        ],
    )?;
    match result {
        Value::Object(value) => Ok(value),
        _ => Err(RuntimeError::type_error(
            "Proxy construct trap must return an object",
        )),
    }
}

fn arguments_array(context: &Object, arguments: &[Value]) -> RuntimeResult<Object> {
    let array = ArrayObject::new_for(context, arguments.len() as u32)?;
    for (index, value) in arguments.iter().cloned().enumerate() {
        array
            .object()
            .create_data_property(JsString::from(index.to_string()).into(), value)?;
    }
    Ok(array.object())
}

fn same_object(left: &Option<Object>, right: &Option<Object>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    }
}
fn invariant(name: &str) -> RuntimeError {
    RuntimeError::type_error(format!("Proxy {name} trap violated a target invariant"))
}
