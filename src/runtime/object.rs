//! Property descriptors and ordinary object internal methods.

use std::hash::{Hash, Hasher};
use std::rc::Rc;

use super::{JsString, Object, RuntimeError, RuntimeResult, Symbol, Value};

/// A JavaScript property key: either a String or a Symbol.
#[derive(Clone, Debug, Eq)]
pub enum PropertyKey {
    /// A UTF-16 string key.
    String(JsString),
    /// A unique symbol key.
    Symbol(Symbol),
}

impl PropertyKey {
    /// Returns the array index represented by this key, when it is a canonical index.
    #[must_use]
    pub fn array_index(&self) -> Option<u32> {
        let Self::String(value) = self else {
            return None;
        };
        let units = value.code_units();
        if units.is_empty() || (units.len() > 1 && units[0] == u16::from(b'0')) {
            return None;
        }
        let mut index = 0_u32;
        for unit in units {
            let digit = u32::from(unit.checked_sub(u16::from(b'0'))?);
            if digit > 9 {
                return None;
            }
            index = index.checked_mul(10)?.checked_add(digit)?;
        }
        (index != u32::MAX).then_some(index)
    }
}

impl From<&str> for PropertyKey {
    fn from(value: &str) -> Self {
        Self::String(JsString::from(value))
    }
}

impl From<JsString> for PropertyKey {
    fn from(value: JsString) -> Self {
        Self::String(value)
    }
}

impl From<Symbol> for PropertyKey {
    fn from(value: Symbol) -> Self {
        Self::Symbol(value)
    }
}

impl PartialEq for PropertyKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::String(left), Self::String(right)) => left == right,
            (Self::Symbol(left), Self::Symbol(right)) => left == right,
            _ => false,
        }
    }
}

impl Hash for PropertyKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::String(value) => {
                0_u8.hash(state);
                value.hash(state);
            }
            Self::Symbol(value) => {
                1_u8.hash(state);
                value.identity().hash(state);
            }
        }
    }
}

/// An ECMAScript Property Descriptor record.
///
/// Outer `Option`s represent whether a field is present. For accessor fields,
/// the inner `Option` distinguishes `undefined` from a callable object.
#[derive(Clone, Debug, Default)]
pub struct PropertyDescriptor {
    /// The `[[Value]]` field.
    pub value: Option<Value>,
    /// The `[[Writable]]` field.
    pub writable: Option<bool>,
    /// The `[[Get]]` field (`Some(None)` represents `undefined`).
    pub get: Option<Option<Object>>,
    /// The `[[Set]]` field (`Some(None)` represents `undefined`).
    pub set: Option<Option<Object>>,
    /// The `[[Enumerable]]` field.
    pub enumerable: Option<bool>,
    /// The `[[Configurable]]` field.
    pub configurable: Option<bool>,
}

impl PropertyDescriptor {
    /// Creates a complete data descriptor.
    #[must_use]
    pub fn data(value: Value, writable: bool, enumerable: bool, configurable: bool) -> Self {
        Self {
            value: Some(value),
            writable: Some(writable),
            enumerable: Some(enumerable),
            configurable: Some(configurable),
            ..Self::default()
        }
    }

    /// Creates a complete accessor descriptor.
    #[must_use]
    pub fn accessor(
        get: Option<Object>,
        set: Option<Object>,
        enumerable: bool,
        configurable: bool,
    ) -> Self {
        Self {
            get: Some(get),
            set: Some(set),
            enumerable: Some(enumerable),
            configurable: Some(configurable),
            ..Self::default()
        }
    }

    /// Returns whether any data-descriptor field is present.
    #[must_use]
    pub fn is_data_descriptor(&self) -> bool {
        self.value.is_some() || self.writable.is_some()
    }

    /// Returns whether any accessor-descriptor field is present.
    #[must_use]
    pub fn is_accessor_descriptor(&self) -> bool {
        self.get.is_some() || self.set.is_some()
    }

    /// Returns whether no data or accessor fields are present.
    #[must_use]
    pub fn is_generic_descriptor(&self) -> bool {
        !self.is_data_descriptor() && !self.is_accessor_descriptor()
    }

    /// Converts a descriptor record to the ordinary object form used by Reflect APIs.
    pub fn to_object(&self) -> RuntimeResult<Object> {
        self.validate()?;
        let object = Object::new();
        if let Some(value) = &self.value {
            object.create_data_property("value".into(), value.clone())?;
        }
        if let Some(writable) = self.writable {
            object.create_data_property("writable".into(), Value::Boolean(writable))?;
        }
        if let Some(get) = &self.get {
            object.create_data_property(
                "get".into(),
                get.clone().map_or(Value::Undefined, Value::Object),
            )?;
        }
        if let Some(set) = &self.set {
            object.create_data_property(
                "set".into(),
                set.clone().map_or(Value::Undefined, Value::Object),
            )?;
        }
        if let Some(enumerable) = self.enumerable {
            object.create_data_property("enumerable".into(), Value::Boolean(enumerable))?;
        }
        if let Some(configurable) = self.configurable {
            object.create_data_property("configurable".into(), Value::Boolean(configurable))?;
        }
        Ok(object)
    }

    /// Implements `ToPropertyDescriptor` for an ordinary object.
    pub fn from_object(object: &Object) -> RuntimeResult<Self> {
        let mut descriptor = Self::default();
        if object.has_property(&"enumerable".into())? {
            descriptor.enumerable = Some(object.get(&"enumerable".into())?.to_boolean());
        }
        if object.has_property(&"configurable".into())? {
            descriptor.configurable = Some(object.get(&"configurable".into())?.to_boolean());
        }
        if object.has_property(&"value".into())? {
            descriptor.value = Some(object.get(&"value".into())?);
        }
        if object.has_property(&"writable".into())? {
            descriptor.writable = Some(object.get(&"writable".into())?.to_boolean());
        }
        if object.has_property(&"get".into())? {
            descriptor.get = Some(callable_or_undefined(object.get(&"get".into())?, "getter")?);
        }
        if object.has_property(&"set".into())? {
            descriptor.set = Some(callable_or_undefined(object.get(&"set".into())?, "setter")?);
        }
        descriptor.validate()?;
        Ok(descriptor)
    }

    fn validate(&self) -> RuntimeResult<()> {
        if self.is_data_descriptor() && self.is_accessor_descriptor() {
            return Err(RuntimeError::type_error(
                "property descriptor cannot be both data and accessor",
            ));
        }
        for (name, callable) in [("getter", &self.get), ("setter", &self.set)] {
            if let Some(Some(callable)) = callable {
                if !callable.is_callable() {
                    return Err(RuntimeError::type_error(format!(
                        "property {name} is not callable"
                    )));
                }
            }
        }
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.value.is_none()
            && self.writable.is_none()
            && self.get.is_none()
            && self.set.is_none()
            && self.enumerable.is_none()
            && self.configurable.is_none()
    }
}

/// Implements ECMAScript `IsCompatiblePropertyDescriptor`.
pub fn is_compatible_property_descriptor(
    extensible: bool,
    descriptor: &PropertyDescriptor,
    current: Option<&PropertyDescriptor>,
) -> RuntimeResult<bool> {
    descriptor.validate()?;
    if let Some(current) = current {
        current.validate()?;
        let current = complete_descriptor(current.clone());
        Ok(validate_and_apply(Some(&current), extensible, descriptor).is_some())
    } else {
        Ok(validate_and_apply(None, extensible, descriptor).is_some())
    }
}

fn callable_or_undefined(value: Value, name: &str) -> RuntimeResult<Option<Object>> {
    match value {
        Value::Undefined => Ok(None),
        Value::Object(object) if object.is_callable() => Ok(Some(object)),
        _ => Err(RuntimeError::type_error(format!(
            "property {name} must be callable or undefined"
        ))),
    }
}

type NativeCall = Rc<dyn Fn(Value, &[Value]) -> RuntimeResult<Value>>;
type NativeConstruct = Rc<dyn Fn(&[Value], Object) -> RuntimeResult<Object>>;

#[derive(Clone)]
struct PropertyEntry {
    key: PropertyKey,
    descriptor: PropertyDescriptor,
}

#[derive(Default)]
pub(super) struct ObjectState {
    prototype: Option<Object>,
    extensible: bool,
    properties: Vec<PropertyEntry>,
    callable: Option<NativeCall>,
    construct: Option<NativeConstruct>,
}

pub(super) fn ordinary_state() -> ObjectState {
    ObjectState {
        extensible: true,
        ..ObjectState::default()
    }
}

impl Object {
    /// Allocates a callable object backed by a native Rust closure.
    #[must_use]
    pub fn native_function(
        callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
    ) -> Self {
        let object = Self::new();
        object.0.state.borrow_mut().callable = Some(Rc::new(callback));
        object
    }

    /// Allocates a constructable object backed by a native Rust closure.
    #[must_use]
    pub fn native_constructor(
        callback: impl Fn(&[Value], Object) -> RuntimeResult<Object> + 'static,
    ) -> Self {
        let object = Self::new();
        object.0.state.borrow_mut().construct = Some(Rc::new(callback));
        object
    }

    /// Allocates an object with both native `[[Call]]` and `[[Construct]]` methods.
    #[must_use]
    pub fn native_callable_constructor(
        call: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
        construct: impl Fn(&[Value], Object) -> RuntimeResult<Object> + 'static,
    ) -> Self {
        let object = Self::native_function(call);
        object.0.state.borrow_mut().construct = Some(Rc::new(construct));
        object
    }

    /// Returns whether this object has a `[[Call]]` internal method.
    #[must_use]
    pub fn is_callable(&self) -> bool {
        self.0.state.borrow().callable.is_some()
    }

    /// Invokes this object's `[[Call]]` internal method.
    pub fn call(&self, this_value: Value, arguments: &[Value]) -> RuntimeResult<Value> {
        let callback = self.0.state.borrow().callable.clone().ok_or_else(|| {
            RuntimeError::type_error("object does not have a [[Call]] internal method")
        })?;
        callback(this_value, arguments)
    }

    /// Returns whether this object has a `[[Construct]]` internal method.
    #[must_use]
    pub fn is_constructor(&self) -> bool {
        self.0.state.borrow().construct.is_some()
    }

    /// Invokes this object's `[[Construct]]` internal method.
    pub fn construct(&self, arguments: &[Value], new_target: Self) -> RuntimeResult<Self> {
        let callback = self.0.state.borrow().construct.clone().ok_or_else(|| {
            RuntimeError::type_error("object does not have a [[Construct]] internal method")
        })?;
        callback(arguments, new_target)
    }

    /// Returns this object's prototype.
    #[must_use]
    pub fn prototype(&self) -> Option<Self> {
        self.0.state.borrow().prototype.clone()
    }

    /// Implements `[[SetPrototypeOf]]`, rejecting cycles and forbidden changes.
    pub fn set_prototype(&self, prototype: Option<Self>) -> bool {
        let current = self.prototype();
        if same_optional_object(&current, &prototype) {
            return true;
        }
        if !self.is_extensible() {
            return false;
        }
        let mut candidate = prototype.clone();
        while let Some(object) = candidate {
            if object == *self {
                return false;
            }
            candidate = object.prototype();
        }
        self.0.state.borrow_mut().prototype = prototype;
        true
    }

    /// Returns whether new own properties may be added.
    #[must_use]
    pub fn is_extensible(&self) -> bool {
        self.0.state.borrow().extensible
    }

    /// Prevents future extensions to this object.
    pub fn prevent_extensions(&self) {
        self.0.state.borrow_mut().extensible = false;
    }

    /// Implements `[[GetOwnProperty]]`.
    #[must_use]
    pub fn get_own_property(&self, key: &PropertyKey) -> Option<PropertyDescriptor> {
        self.0
            .state
            .borrow()
            .properties
            .iter()
            .find(|entry| entry.key == *key)
            .map(|entry| entry.descriptor.clone())
    }

    /// Implements `[[DefineOwnProperty]]` and descriptor compatibility rules.
    pub fn define_own_property(
        &self,
        key: PropertyKey,
        descriptor: PropertyDescriptor,
    ) -> RuntimeResult<bool> {
        descriptor.validate()?;
        let mut state = self.0.state.borrow_mut();
        let position = state.properties.iter().position(|entry| entry.key == key);
        let current = position.map(|index| &state.properties[index].descriptor);
        let Some(updated) = validate_and_apply(current, state.extensible, &descriptor) else {
            return Ok(false);
        };
        if let Some(position) = position {
            state.properties[position].descriptor = updated;
        } else {
            state.properties.push(PropertyEntry {
                key,
                descriptor: updated,
            });
        }
        Ok(true)
    }

    /// Defines a writable, enumerable, configurable own data property.
    pub fn create_data_property(&self, key: PropertyKey, value: Value) -> RuntimeResult<bool> {
        self.define_own_property(key, PropertyDescriptor::data(value, true, true, true))
    }

    /// Implements `HasProperty`, including the prototype chain.
    pub fn has_property(&self, key: &PropertyKey) -> RuntimeResult<bool> {
        if self.get_own_property(key).is_some() {
            return Ok(true);
        }
        self.prototype()
            .map_or(Ok(false), |prototype| prototype.has_property(key))
    }

    /// Implements ordinary `[[Get]]` with this object as the receiver.
    pub fn get(&self, key: &PropertyKey) -> RuntimeResult<Value> {
        self.get_with_receiver(key, Value::Object(self.clone()))
    }

    /// Implements ordinary `[[Get]]` with an explicit receiver.
    pub fn get_with_receiver(&self, key: &PropertyKey, receiver: Value) -> RuntimeResult<Value> {
        let Some(descriptor) = self.get_own_property(key) else {
            return self.prototype().map_or(Ok(Value::Undefined), |prototype| {
                prototype.get_with_receiver(key, receiver)
            });
        };
        if descriptor.is_data_descriptor() {
            return Ok(descriptor.value.unwrap_or(Value::Undefined));
        }
        let Some(getter) = descriptor.get.flatten() else {
            return Ok(Value::Undefined);
        };
        getter.call(receiver, &[])
    }

    /// Implements ordinary `[[Set]]` with this object as the receiver.
    pub fn set(&self, key: &PropertyKey, value: Value) -> RuntimeResult<bool> {
        self.set_with_receiver(key, value, Value::Object(self.clone()))
    }

    /// Implements ordinary `[[Set]]` with an explicit receiver.
    pub fn set_with_receiver(
        &self,
        key: &PropertyKey,
        value: Value,
        receiver: Value,
    ) -> RuntimeResult<bool> {
        let descriptor = self.get_own_property(key);
        let Some(descriptor) = descriptor else {
            if let Some(prototype) = self.prototype() {
                return prototype.set_with_receiver(key, value, receiver);
            }
            let Value::Object(receiver) = receiver else {
                return Ok(false);
            };
            return receiver.create_data_property(key.clone(), value);
        };
        if descriptor.is_data_descriptor() {
            if descriptor.writable != Some(true) {
                return Ok(false);
            }
            let Value::Object(receiver) = receiver else {
                return Ok(false);
            };
            if let Some(existing) = receiver.get_own_property(key) {
                if existing.is_accessor_descriptor() || existing.writable != Some(true) {
                    return Ok(false);
                }
                return receiver.define_own_property(
                    key.clone(),
                    PropertyDescriptor {
                        value: Some(value),
                        ..PropertyDescriptor::default()
                    },
                );
            }
            return receiver.create_data_property(key.clone(), value);
        }
        let Some(setter) = descriptor.set.flatten() else {
            return Ok(false);
        };
        setter.call(receiver, &[value])?;
        Ok(true)
    }

    /// Implements ordinary `[[Delete]]`.
    pub fn delete(&self, key: &PropertyKey) -> bool {
        let mut state = self.0.state.borrow_mut();
        let Some(position) = state.properties.iter().position(|entry| entry.key == *key) else {
            return true;
        };
        if state.properties[position].descriptor.configurable != Some(true) {
            return false;
        }
        state.properties.remove(position);
        true
    }

    /// Implements ordinary `[[OwnPropertyKeys]]` ordering.
    #[must_use]
    pub fn own_property_keys(&self) -> Vec<PropertyKey> {
        let state = self.0.state.borrow();
        let mut indices = state
            .properties
            .iter()
            .filter_map(|entry| {
                entry
                    .key
                    .array_index()
                    .map(|index| (index, entry.key.clone()))
            })
            .collect::<Vec<_>>();
        indices.sort_unstable_by_key(|(index, _)| *index);
        let strings = state
            .properties
            .iter()
            .filter_map(|entry| match &entry.key {
                PropertyKey::String(_) if entry.key.array_index().is_none() => {
                    Some(entry.key.clone())
                }
                _ => None,
            });
        let symbols = state
            .properties
            .iter()
            .filter_map(|entry| match &entry.key {
                PropertyKey::Symbol(_) => Some(entry.key.clone()),
                PropertyKey::String(_) => None,
            });
        indices
            .into_iter()
            .map(|(_, key)| key)
            .chain(strings)
            .chain(symbols)
            .collect()
    }
}

impl Value {
    /// Implements ECMAScript `ToPropertyKey`.
    pub fn to_property_key(&self) -> RuntimeResult<PropertyKey> {
        let key = self.to_primitive(super::PrimitiveHint::String)?;
        if let Self::Symbol(symbol) = key {
            Ok(PropertyKey::Symbol(symbol))
        } else {
            key.to_js_string().map(PropertyKey::String)
        }
    }
}

fn same_optional_object(left: &Option<Object>, right: &Option<Object>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    }
}

fn validate_and_apply(
    current: Option<&PropertyDescriptor>,
    extensible: bool,
    descriptor: &PropertyDescriptor,
) -> Option<PropertyDescriptor> {
    let Some(current) = current else {
        return extensible.then(|| complete_descriptor(descriptor.clone()));
    };
    if descriptor.is_empty() {
        return Some(current.clone());
    }
    if current.configurable == Some(false)
        && (descriptor.configurable == Some(true)
            || descriptor
                .enumerable
                .is_some_and(|value| Some(value) != current.enumerable))
    {
        return None;
    }

    let current_data = current.is_data_descriptor();
    let descriptor_data = descriptor.is_data_descriptor();
    let descriptor_accessor = descriptor.is_accessor_descriptor();
    let mut updated = current.clone();
    if !descriptor.is_generic_descriptor() && current_data != descriptor_data {
        if current.configurable == Some(false) {
            return None;
        }
        updated = if descriptor_accessor {
            PropertyDescriptor::accessor(None, None, false, current.configurable.unwrap_or(false))
        } else {
            PropertyDescriptor::data(
                Value::Undefined,
                false,
                false,
                current.configurable.unwrap_or(false),
            )
        };
        updated.enumerable = current.enumerable;
    } else if current_data && descriptor_data && current.configurable == Some(false) {
        if current.writable == Some(false)
            && (descriptor.writable == Some(true)
                || descriptor
                    .value
                    .as_ref()
                    .is_some_and(|value| !value.same_value(current.value.as_ref().unwrap())))
        {
            return None;
        }
    } else if current.is_accessor_descriptor()
        && descriptor_accessor
        && current.configurable == Some(false)
        && (descriptor
            .get
            .as_ref()
            .is_some_and(|get| !same_optional_object(get, current.get.as_ref().unwrap()))
            || descriptor
                .set
                .as_ref()
                .is_some_and(|set| !same_optional_object(set, current.set.as_ref().unwrap())))
    {
        return None;
    }

    if let Some(value) = &descriptor.value {
        updated.value = Some(value.clone());
    }
    if let Some(value) = descriptor.writable {
        updated.writable = Some(value);
    }
    if let Some(value) = &descriptor.get {
        updated.get = Some(value.clone());
    }
    if let Some(value) = &descriptor.set {
        updated.set = Some(value.clone());
    }
    if let Some(value) = descriptor.enumerable {
        updated.enumerable = Some(value);
    }
    if let Some(value) = descriptor.configurable {
        updated.configurable = Some(value);
    }
    Some(updated)
}

fn complete_descriptor(mut descriptor: PropertyDescriptor) -> PropertyDescriptor {
    if descriptor.is_accessor_descriptor() {
        descriptor.get.get_or_insert(None);
        descriptor.set.get_or_insert(None);
    } else {
        descriptor.value.get_or_insert(Value::Undefined);
        descriptor.writable.get_or_insert(false);
    }
    descriptor.enumerable.get_or_insert(false);
    descriptor.configurable.get_or_insert(false);
    descriptor
}
