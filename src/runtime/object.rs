//! Property descriptors and ordinary object internal methods.

use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use super::stored::{StoredDescriptor, StoredObject, StoredValue};
use super::{JsString, Object, RuntimeError, RuntimeResult, Symbol, Value};

// Native callbacks and the current AST function implementation recurse through
// Rust. Share one per-thread budget across calls, construction and VM reentry.
// This conservative bound also applies on the supported debug/MSRV builds.
const MAX_CALL_DEPTH: usize = 32;
thread_local! {
    static CALL_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct CallDepthGuard;

impl CallDepthGuard {
    fn enter() -> RuntimeResult<Self> {
        CALL_DEPTH.with(|depth| {
            if depth.get() >= MAX_CALL_DEPTH {
                return Err(RuntimeError::range_error("maximum call depth exceeded"));
            }
            depth.set(depth.get() + 1);
            Ok(Self)
        })
    }
}

impl Drop for CallDepthGuard {
    fn drop(&mut self) {
        CALL_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

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
    descriptor: StoredDescriptor,
}

#[derive(Default)]
pub(super) struct ObjectState {
    prototype: Option<StoredObject>,
    extensible: bool,
    properties: Vec<PropertyEntry>,
    // A position hint only. Bounds and the complete key are checked on every
    // read, so deleting/reordering properties never returns a stale descriptor.
    last_property: Cell<usize>,
    callable: Option<NativeCall>,
    pub(super) bound_target: Option<StoredObject>,
    construct: Option<NativeConstruct>,
    // The own data property already exists. Only its initial object value is
    // deferred until an operation observes or redefines that descriptor.
    lazy_function_prototype: Option<StoredObject>,
    pub(super) array: bool,
    pub(super) elements: Option<Box<super::elements::Elements<StoredDescriptor>>>,
    pub(super) buffer: Option<super::indexed::BufferSlot>,
    pub(super) view: Option<super::indexed::ViewSlot>,
    pub(super) string_data: Option<JsString>,
    pub(super) regexp: Option<super::regexp::RegExpSlot>,
    pub(super) primitive_data: Option<Value>,
    pub(super) map_data: Option<super::collections::MapSlot>,
    pub(super) set_data: Option<super::collections::SetSlot>,
    pub(super) weak_map_data: Option<super::collections::WeakMapSlot>,
    pub(super) weak_set_data: Option<super::collections::WeakSetSlot>,
    pub(super) date_value: Option<f64>,
    pub(super) iterator: Option<Box<super::iterator::IteratorSlot>>,
    pub(super) proxy: Option<super::proxy::ProxySlot>,
}

pub(super) fn ordinary_state() -> ObjectState {
    ObjectState {
        extensible: true,
        ..ObjectState::default()
    }
}

impl Object {
    pub(crate) fn own_element_value(&self, index: u32) -> Option<Value> {
        let state = self.0.state.borrow();
        if !state.array
            || state.proxy.is_some()
            || state.string_data.is_some()
            || state.view.is_some()
            || state.buffer.is_some()
        {
            return None;
        }
        let descriptor = state.elements.as_ref()?.get(index)?;
        descriptor.value()?.load(self).ok()
    }

    // A hint identifies a position, never a cached Value or assumed shape.
    // Validate the current key/descriptor under one borrow before returning.
    pub(crate) fn own_data_value_with_hint(
        &self,
        key: &PropertyKey,
        hint: Option<usize>,
    ) -> Option<(Value, usize)> {
        let state = self.0.state.borrow();
        if state.proxy.is_some()
            || state.lazy_function_prototype.is_some()
            || state.string_data.is_some()
            || (state.elements.is_some() && key.array_index().is_some())
            || state.view.is_some()
            || state.buffer.is_some()
        {
            return None;
        }
        let (index, entry) = hint
            .and_then(|index| {
                state
                    .properties
                    .get(index)
                    .filter(|entry| entry.key == *key)
                    .map(|entry| (index, entry))
            })
            .or_else(|| {
                state
                    .properties
                    .iter()
                    .enumerate()
                    .find(|(_, entry)| entry.key == *key)
            })?;
        entry
            .descriptor
            .value()?
            .load(self)
            .ok()
            .map(|value| (value, index))
    }

    pub(super) fn install_lazy_function_prototype(
        &self,
        object_prototype: Object,
    ) -> RuntimeResult<()> {
        self.define_own_property(
            "prototype".into(),
            PropertyDescriptor::data(Value::Undefined, true, false, false),
        )?;
        self.0.state.borrow_mut().lazy_function_prototype =
            Some(StoredObject::store(self, &object_prototype)?);
        Ok(())
    }

    fn materialize_function_prototype(&self, key: &PropertyKey) -> RuntimeResult<()> {
        if !matches!(key, PropertyKey::String(name) if name.code_units().iter().copied().eq("prototype".encode_utf16()))
        {
            return Ok(());
        }
        let Some(parent) = self.0.state.borrow_mut().lazy_function_prototype.take() else {
            return Ok(());
        };
        let parent = parent.load(self)?;
        let prototype = self.0.runtime().map_or_else(Object::new, Object::new_in);
        prototype.set_prototype(Some(parent));
        prototype.define_own_property(
            "constructor".into(),
            PropertyDescriptor::data(Value::Object(self.clone()), true, false, true),
        )?;
        // Do not use ordinary DefineOwnProperty here: the logical initial value
        // exists even if its property or the function was made nonwritable or
        // nonextensible. Preserve all attributes and insertion order.
        let mut state = self.0.state.borrow_mut();
        let entry = state
            .properties
            .iter_mut()
            .find(|entry| entry.key == *key)
            .expect("lazy function prototype property");
        entry
            .descriptor
            .set_value(StoredValue::store(self, &Value::Object(prototype))?);
        Ok(())
    }

    pub(crate) fn set_native_call(
        &self,
        callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
    ) {
        self.0.state.borrow_mut().callable = Some(Rc::new(callback));
    }

    pub(crate) fn set_native_construct(
        &self,
        callback: impl Fn(&[Value], Object) -> RuntimeResult<Object> + 'static,
    ) {
        self.0.state.borrow_mut().construct = Some(Rc::new(callback));
    }

    pub(crate) fn clear_native_construct(&self) {
        self.0.state.borrow_mut().construct = None;
    }

    pub(crate) fn clear_native_call(&self) {
        self.0.state.borrow_mut().callable = None;
    }

    /// Allocates a callable object backed by a native Rust closure.
    #[must_use]
    pub fn native_function(
        callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
    ) -> Self {
        let object = Self::new();
        object.0.state.borrow_mut().callable = Some(Rc::new(callback));
        object
    }

    pub(super) fn native_function_for(
        context: &Object,
        callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
    ) -> Self {
        let object = Self::new_for(context);
        object.set_native_call(callback);
        object
    }

    pub(super) fn native_constructor_for(
        context: &Object,
        callback: impl Fn(&[Value], Object) -> RuntimeResult<Object> + 'static,
    ) -> Self {
        let object = Self::new_for(context);
        object.set_native_construct(callback);
        object
    }

    pub(super) fn native_callable_constructor_for(
        context: &Object,
        call: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
        construct: impl Fn(&[Value], Object) -> RuntimeResult<Object> + 'static,
    ) -> Self {
        let object = Self::native_function_for(context, call);
        object.set_native_construct(construct);
        object
    }

    pub(super) fn native_function_with_metadata(
        prototype: &Object,
        name: &str,
        length: u32,
        callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
    ) -> Self {
        // This fresh ordinary object has no existing descriptors, exotic hooks,
        // or observable identity. Seed complete properties in insertion order
        // without descriptor validation or a Vec growth allocation for four
        // entries when only two are needed. Later mutations use normal methods.
        let object = prototype.0.runtime().map_or_else(Self::new, Self::new_in);
        {
            let mut state = object.0.state.borrow_mut();
            state.prototype =
                Some(StoredObject::store(&object, prototype).expect("native prototype context"));
            state.callable = Some(Rc::new(callback));
            state.properties = vec![
                PropertyEntry {
                    key: "name".into(),
                    descriptor: StoredDescriptor::Data {
                        value: StoredValue::String(name.into()),
                        writable: false,
                        enumerable: false,
                        configurable: true,
                    },
                },
                PropertyEntry {
                    key: "length".into(),
                    descriptor: StoredDescriptor::Data {
                        value: StoredValue::Number(f64::from(length)),
                        writable: false,
                        enumerable: false,
                        configurable: true,
                    },
                },
            ];
        }
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
    ///
    /// Calls and construction share a per-thread nesting limit of 32. Exceeding
    /// the limit returns RangeError; normal and abrupt exits release the budget.
    pub fn call(&self, this_value: Value, arguments: &[Value]) -> RuntimeResult<Value> {
        let callback = self.0.state.borrow().callable.clone().ok_or_else(|| {
            RuntimeError::type_error("object does not have a [[Call]] internal method")
        })?;
        if self.0.runtime().is_some() {
            if let Value::Object(value) = &this_value {
                StoredObject::check_context(self, value)?;
            }
            for value in arguments {
                if let Value::Object(value) = value {
                    StoredObject::check_context(self, value)?;
                }
            }
        }
        let _depth = CallDepthGuard::enter()?;
        let result = callback(this_value, arguments);
        if let Err(error) = &result {
            self.check_abrupt_context(error)?;
        }
        let result = result?;
        if self.0.runtime().is_some() {
            if let Value::Object(value) = &result {
                StoredObject::check_context(self, value)?;
            }
        }
        Ok(result)
    }

    /// Returns whether this object has a `[[Construct]]` internal method.
    #[must_use]
    pub fn is_constructor(&self) -> bool {
        self.0.state.borrow().construct.is_some()
    }

    /// Invokes this object's `[[Construct]]` internal method.
    /// Shares the nesting limit documented on [`Self::call`].
    pub fn construct(&self, arguments: &[Value], new_target: Self) -> RuntimeResult<Self> {
        let callback = self.0.state.borrow().construct.clone().ok_or_else(|| {
            RuntimeError::type_error("object does not have a [[Construct]] internal method")
        })?;
        if self.0.runtime().is_some() {
            StoredObject::check_context(self, &new_target)?;
            for value in arguments {
                if let Value::Object(value) = value {
                    StoredObject::check_context(self, value)?;
                }
            }
        }
        let _depth = CallDepthGuard::enter()?;
        let result = callback(arguments, new_target);
        if let Err(error) = &result {
            self.check_abrupt_context(error)?;
        }
        let result = result?;
        if self.0.runtime().is_some() {
            StoredObject::check_context(self, &result)?;
        }
        Ok(result)
    }

    /// Initializes an unexposed fresh ordinary object's prototype. Creation is
    /// not SetPrototypeOf: no cycle walk or user Proxy callbacks are required.
    pub(super) fn initialize_prototype(&self, prototype: Option<Self>) {
        debug_assert!(self.0.state.borrow().prototype.is_none());
        self.0.state.borrow_mut().prototype = prototype
            .as_ref()
            .map(|value| StoredObject::store(self, value))
            .transpose()
            .expect("fresh prototype allocation context");
    }

    /// Returns this object's prototype.
    #[must_use]
    pub fn prototype(&self) -> Option<Self> {
        self.try_prototype().ok().flatten()
    }

    /// Fallible `[[GetPrototypeOf]]`, including Proxy traps and invariants.
    pub fn try_prototype(&self) -> RuntimeResult<Option<Self>> {
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::get_prototype_of(self);
        }
        self.0
            .state
            .borrow()
            .prototype
            .as_ref()
            .map(|value| value.load(self))
            .transpose()
    }

    /// Implements `[[SetPrototypeOf]]`, rejecting cycles and forbidden changes.
    pub fn set_prototype(&self, prototype: Option<Self>) -> bool {
        self.try_set_prototype(prototype).unwrap_or(false)
    }

    /// Fallible `[[SetPrototypeOf]]`, including Proxy traps and invariants.
    pub fn try_set_prototype(&self, prototype: Option<Self>) -> RuntimeResult<bool> {
        let stored = prototype
            .as_ref()
            .map(|value| StoredObject::store(self, value))
            .transpose()?;
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::set_prototype_of(self, prototype);
        }
        let current = self.prototype();
        if same_optional_object(&current, &prototype) {
            return Ok(true);
        }
        if !self.is_extensible() {
            return Ok(false);
        }
        let mut candidate = prototype.clone();
        while let Some(object) = candidate {
            if object == *self {
                return Ok(false);
            }
            candidate = object.prototype();
        }
        self.0.state.borrow_mut().prototype = stored;
        Ok(true)
    }

    /// Returns whether new own properties may be added.
    #[must_use]
    pub fn is_extensible(&self) -> bool {
        self.try_is_extensible().unwrap_or(false)
    }

    /// Fallible `[[IsExtensible]]`, including Proxy invariants.
    pub fn try_is_extensible(&self) -> RuntimeResult<bool> {
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::is_extensible(self);
        }
        Ok(self.0.state.borrow().extensible)
    }

    /// Prevents future extensions to this object.
    pub fn prevent_extensions(&self) {
        let _ = self.try_prevent_extensions();
    }

    /// Fallible `[[PreventExtensions]]`, including Proxy traps and invariants.
    pub fn try_prevent_extensions(&self) -> RuntimeResult<bool> {
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::prevent_extensions(self);
        }
        self.0.state.borrow_mut().extensible = false;
        Ok(true)
    }

    /// Implements `[[GetOwnProperty]]`.
    #[must_use]
    pub fn get_own_property(&self, key: &PropertyKey) -> Option<PropertyDescriptor> {
        self.try_get_own_property(key).ok().flatten()
    }

    /// Fallible `[[GetOwnProperty]]`, including Proxy invariants.
    pub fn try_get_own_property(
        &self,
        key: &PropertyKey,
    ) -> RuntimeResult<Option<PropertyDescriptor>> {
        let mut state = self.0.state.borrow();
        if state.proxy.is_some() {
            drop(state);
            return super::proxy::get_own_property(self, key);
        }
        if state.lazy_function_prototype.is_some() {
            // Materialization can mutate this object. Release the borrow first;
            // ordinary objects keep one read borrow for the complete lookup.
            drop(state);
            self.materialize_function_prototype(key)?;
            state = self.0.state.borrow();
        }
        if let Some(string) = &state.string_data {
            if matches!(key, PropertyKey::String(name) if name == &JsString::from("length")) {
                return Ok(Some(PropertyDescriptor::data(
                    Value::Number(string.len() as f64),
                    false,
                    false,
                    false,
                )));
            }
            if let Some(index) = key.array_index() {
                if let Some(unit) = string.code_units().get(index as usize) {
                    return Ok(Some(PropertyDescriptor::data(
                        Value::String(JsString::from_code_units(vec![*unit])),
                        false,
                        true,
                        false,
                    )));
                }
            }
        }
        if let Some(elements) = &state.elements {
            if let Some(index) = key.array_index() {
                return elements
                    .get(index)
                    .map(|descriptor| descriptor.load(self))
                    .transpose();
            }
        }
        if let Some(entry) = state
            .properties
            .get(state.last_property.get())
            .filter(|entry| entry.key == *key)
        {
            return Ok(Some(entry.descriptor.load(self)?));
        }
        let found = state
            .properties
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.key == *key);
        found
            .map(|(index, entry)| {
                state.last_property.set(index);
                entry.descriptor.load(self)
            })
            .transpose()
    }

    /// Implements `[[DefineOwnProperty]]` and descriptor compatibility rules.
    pub fn define_own_property(
        &self,
        key: PropertyKey,
        descriptor: PropertyDescriptor,
    ) -> RuntimeResult<bool> {
        descriptor.validate()?;
        if let Some(Value::Object(value)) = &descriptor.value {
            StoredObject::check_context(self, value)?;
        }
        for value in [&descriptor.get, &descriptor.set]
            .into_iter()
            .flatten()
            .flatten()
        {
            StoredObject::check_context(self, value)?;
        }
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::define_own_property(self, key, descriptor);
        }
        self.materialize_function_prototype(&key)?;
        if self.0.state.borrow().string_data.is_some() {
            if let Some(current) = self.string_exotic_property(&key) {
                return is_compatible_property_descriptor(false, &descriptor, Some(&current));
            }
        }
        if self.0.state.borrow().array {
            if matches!(&key, PropertyKey::String(name) if name == &JsString::from("length")) {
                return self.define_array_length(descriptor);
            }
            if let Some(index) = key.array_index() {
                let old_length = self.array_length();
                if index >= old_length && !self.array_length_writable() {
                    return Ok(false);
                }
                let accepted = self.define_ordinary_property(key, descriptor)?;
                if accepted && index >= old_length {
                    self.set_array_length_value(index + 1);
                }
                return Ok(accepted);
            }
        }
        self.define_ordinary_property(key, descriptor)
    }

    fn define_ordinary_property(
        &self,
        key: PropertyKey,
        descriptor: PropertyDescriptor,
    ) -> RuntimeResult<bool> {
        let mut state = self.0.state.borrow_mut();
        if state.array {
            if let Some(index) = key.array_index() {
                let elements = state.elements.as_ref().expect("array elements");
                let current = elements
                    .get(index)
                    .map(|value| value.load(self))
                    .transpose()?;
                let Some(updated) =
                    validate_and_apply(current.as_ref(), state.extensible, &descriptor)
                else {
                    return Ok(false);
                };
                state
                    .elements
                    .as_mut()
                    .unwrap()
                    .insert(index, StoredDescriptor::store(self, &updated)?);
                return Ok(true);
            }
        }
        let position = state.properties.iter().position(|entry| entry.key == key);
        let current = position
            .map(|index| state.properties[index].descriptor.load(self))
            .transpose()?;
        let Some(updated) = validate_and_apply(current.as_ref(), state.extensible, &descriptor)
        else {
            return Ok(false);
        };
        let updated = StoredDescriptor::store(self, &updated)?;
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

    // No user code runs between these guards and the writes. Reject every
    // prototype with indexed properties or exotic indexed/proxy behavior so
    // inherited setters and readonly descriptors retain the generic Set path.
    pub(super) fn try_push_array_values(&self, length: u64, values: &[Value]) -> Option<u32> {
        let length = u32::try_from(length).ok()?;
        let count = u32::try_from(values.len()).ok()?;
        let end = length.checked_add(count)?;
        let (length_slot, mut prototype) = {
            let state = self.0.state.borrow();
            if !state.array
                || state.proxy.is_some()
                || state.string_data.is_some()
                || state.view.is_some()
                || state.buffer.is_some()
                || (!values.is_empty() && !state.extensible)
            {
                return None;
            }
            let slot = state.properties.iter().position(|entry| {
                matches!(&entry.key, PropertyKey::String(name) if name.code_units().iter().copied().eq("length".encode_utf16()))
            })?;
            let descriptor = &state.properties[slot].descriptor;
            if !descriptor.is_data_descriptor()
                || descriptor.writable() != Some(true)
                || !matches!(descriptor.value(), Some(StoredValue::Number(n)) if *n == f64::from(length))
            {
                return None;
            }
            let elements = state.elements.as_ref()?;
            if (length..end).any(|index| elements.get(index).is_some()) {
                return None;
            }
            (
                slot,
                state
                    .prototype
                    .as_ref()
                    .map(|value| value.load(self))
                    .transpose()
                    .ok()?,
            )
        };
        if !values.is_empty() {
            let mut depth = 0;
            while let Some(object) = prototype {
                // A bounded conservative check also handles malformed host
                // prototype cycles without changing their generic behavior.
                depth += 1;
                if depth > 64 {
                    return None;
                }
                let state = object.0.state.borrow();
                if state.proxy.is_some()
                    || state.string_data.is_some()
                    || state.view.is_some()
                    || state.buffer.is_some()
                    || state
                        .elements
                        .as_ref()
                        .is_some_and(|elements| !elements.is_empty())
                    || state
                        .properties
                        .iter()
                        .any(|entry| entry.key.array_index().is_some())
                {
                    return None;
                }
                prototype = state
                    .prototype
                    .as_ref()
                    .map(|value| value.load(&object))
                    .transpose()
                    .ok()?;
            }
        }
        for value in values {
            if let Value::Object(value) = value {
                StoredObject::store(self, value).ok()?;
            }
        }
        let mut state = self.0.state.borrow_mut();
        let ObjectState {
            properties,
            elements,
            ..
        } = &mut *state;
        let elements = elements.as_mut()?;
        for (offset, value) in values.iter().enumerate() {
            let index = length + offset as u32;
            elements.insert(
                index,
                StoredDescriptor::Data {
                    value: StoredValue::store(self, value).ok()?,
                    writable: true,
                    enumerable: true,
                    configurable: true,
                },
            );
            // Maintain Array length after every inserted index, as generic
            // DefineOwnProperty does, including if host allocation unwinds.
            properties[length_slot]
                .descriptor
                .set_value(StoredValue::Number(f64::from(index + 1)));
        }
        Some(end)
    }

    fn ordinary_array_length_info(&self) -> Option<(u32, bool)> {
        let state = self.0.state.borrow();
        if !state.array
            || state.proxy.is_some()
            || state.string_data.is_some()
            || state.view.is_some()
            || state.buffer.is_some()
        {
            return None;
        }
        let entry = state.properties.iter().find(|entry| {
            matches!(&entry.key, PropertyKey::String(name) if name.code_units().iter().copied().eq("length".encode_utf16()))
        })?;
        let Some(StoredValue::Number(length)) = entry.descriptor.value() else {
            return None;
        };
        Some((*length as u32, entry.descriptor.writable() == Some(true)))
    }

    pub(super) fn array_length(&self) -> u32 {
        if let Some((length, _)) = self.ordinary_array_length_info() {
            return length;
        }
        match self
            .get_own_property(&"length".into())
            .and_then(|d| d.value)
        {
            Some(Value::Number(value)) => value as u32,
            _ => 0,
        }
    }

    fn array_length_writable(&self) -> bool {
        if let Some((_, writable)) = self.ordinary_array_length_info() {
            return writable;
        }
        self.get_own_property(&"length".into())
            .and_then(|descriptor| descriptor.writable)
            .unwrap_or(false)
    }

    fn set_array_length_value(&self, length: u32) {
        let mut state = self.0.state.borrow_mut();
        let entry = state
            .properties
            .iter_mut()
            .find(|entry| entry.key == PropertyKey::from("length"))
            .expect("array length property");
        entry
            .descriptor
            .set_value(StoredValue::Number(f64::from(length)));
    }

    fn define_array_length(&self, mut descriptor: PropertyDescriptor) -> RuntimeResult<bool> {
        let Some(Value::Number(number)) = descriptor.value.as_ref() else {
            return self.define_ordinary_property("length".into(), descriptor);
        };
        if !number.is_finite()
            || *number < 0.0
            || number.fract() != 0.0
            || *number > f64::from(u32::MAX)
        {
            return Err(RuntimeError::range_error("invalid array length"));
        }
        let new_length = *number as u32;
        // Normalize -0, including when the old length is not writable.
        descriptor.value = Some(Value::Number(f64::from(new_length)));
        let old_length = self.array_length();
        if new_length >= old_length {
            return self.define_ordinary_property("length".into(), descriptor);
        }
        if !self.array_length_writable() {
            return Ok(false);
        }
        let make_readonly = descriptor.writable == Some(false);
        if make_readonly {
            descriptor.writable = Some(true);
        }
        // Reject invalid attributes before deleting any elements. Defer making
        // length read-only until any partial shrink has restored its value.
        if !self.define_ordinary_property("length".into(), descriptor)? {
            return Ok(false);
        }
        let blocked = self
            .0
            .state
            .borrow_mut()
            .elements
            .as_mut()
            .expect("array elements")
            .shrink(new_length);
        if let Some(index) = blocked {
            self.set_array_length_value(index + 1);
        }
        if make_readonly {
            self.define_ordinary_property(
                "length".into(),
                PropertyDescriptor {
                    writable: Some(false),
                    ..PropertyDescriptor::default()
                },
            )?;
        }
        Ok(blocked.is_none())
    }

    /// Defines a writable, enumerable, configurable own data property.
    pub fn create_data_property(&self, key: PropertyKey, value: Value) -> RuntimeResult<bool> {
        self.define_own_property(key, PropertyDescriptor::data(value, true, true, true))
    }

    /// Implements `HasProperty`, including the prototype chain.
    pub fn has_property(&self, key: &PropertyKey) -> RuntimeResult<bool> {
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::has(self, key);
        }
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
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::get(self, key, receiver);
        }
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
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::set(self, key, value, receiver);
        }
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
        self.try_delete(key).unwrap_or(false)
    }

    /// Fallible `[[Delete]]`, including Proxy invariants.
    pub fn try_delete(&self, key: &PropertyKey) -> RuntimeResult<bool> {
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::delete(self, key);
        }
        if self.string_exotic_property(key).is_some() {
            return Ok(false);
        }
        let mut state = self.0.state.borrow_mut();
        if let Some(elements) = &mut state.elements {
            if let Some(index) = key.array_index() {
                if elements
                    .get(index)
                    .is_some_and(|d| d.configurable() != Some(true))
                {
                    return Ok(false);
                }
                elements.remove(index);
                return Ok(true);
            }
        }
        let Some(position) = state.properties.iter().position(|entry| entry.key == *key) else {
            return Ok(true);
        };
        if state.properties[position].descriptor.configurable() != Some(true) {
            return Ok(false);
        }
        state.properties.remove(position);
        Ok(true)
    }

    /// Implements ordinary `[[OwnPropertyKeys]]` ordering.
    #[must_use]
    pub fn own_property_keys(&self) -> Vec<PropertyKey> {
        self.try_own_property_keys().unwrap_or_default()
    }

    /// Fallible `[[OwnPropertyKeys]]`, including Proxy invariants.
    pub fn try_own_property_keys(&self) -> RuntimeResult<Vec<PropertyKey>> {
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::own_keys(self);
        }
        let state = self.0.state.borrow();
        let string_keys = state.string_data.as_ref().map_or_else(Vec::new, |string| {
            (0..string.len())
                .map(|index| PropertyKey::String(JsString::from(index.to_string())))
                .chain(std::iter::once(PropertyKey::from("length")))
                .collect()
        });
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
        if let Some(elements) = &state.elements {
            indices.extend(
                elements
                    .indices()
                    .into_iter()
                    .map(|index| (index, PropertyKey::from(JsString::from(index.to_string())))),
            );
        }
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
        Ok(string_keys
            .into_iter()
            .chain(
                indices
                    .into_iter()
                    .map(|(_, key)| key)
                    .chain(strings)
                    .chain(symbols),
            )
            .collect())
    }

    fn string_exotic_property(&self, key: &PropertyKey) -> Option<PropertyDescriptor> {
        let state = self.0.state.borrow();
        let string = state.string_data.as_ref()?;
        if matches!(key, PropertyKey::String(name) if name == &JsString::from("length")) {
            return Some(PropertyDescriptor::data(
                Value::Number(string.len() as f64),
                false,
                false,
                false,
            ));
        }
        let unit = string.code_units().get(key.array_index()? as usize)?;
        Some(PropertyDescriptor::data(
            Value::String(JsString::from_code_units(vec![*unit])),
            false,
            true,
            false,
        ))
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

impl super::ObjectData {
    // Allocation collection remains disabled until the remaining payloads have
    // non-owning, typed captures and active evaluator roots.
    pub(super) fn trace_object_edges(&self, tracer: &mut crate::gc::Tracer<'_>) {
        let state = self.state.borrow();
        if let Some(prototype) = &state.prototype {
            prototype.trace(tracer);
        }
        if let Some(parent) = &state.lazy_function_prototype {
            parent.trace(tracer);
        }
        if let Some(target) = &state.bound_target {
            target.trace(tracer);
        }
        for entry in &state.properties {
            entry.descriptor.trace(tracer);
        }
        if let Some(elements) = &state.elements {
            elements.for_each(|descriptor| descriptor.trace(tracer));
        }
    }
}

#[cfg(test)]
mod own_data_hint_tests {
    use super::*;

    #[test]
    fn descriptor_lookup_hint_rechecks_keys_and_reads_current_descriptors() {
        let object = Object::new();
        for (key, value) in [("a", 1.0), ("b", 2.0), ("c", 3.0)] {
            object
                .create_data_property(key.into(), Value::Number(value))
                .unwrap();
        }
        assert!(
            object
                .get(&"c".into())
                .unwrap()
                .same_value(&Value::Number(3.0))
        );
        assert!(object.delete(&"a".into()));
        assert!(
            object
                .get(&"c".into())
                .unwrap()
                .same_value(&Value::Number(3.0))
        );
        assert!(object.delete(&"c".into()));
        assert!(matches!(object.get(&"c".into()).unwrap(), Value::Undefined));
        object
            .create_data_property("d".into(), Value::Number(4.0))
            .unwrap();
        assert!(matches!(object.get(&"c".into()).unwrap(), Value::Undefined));

        let getter = Object::native_function(|_, _| Ok(Value::Number(9.0)));
        object.get(&"b".into()).unwrap();
        object
            .define_own_property(
                "b".into(),
                PropertyDescriptor::accessor(Some(getter), None, true, true),
            )
            .unwrap();
        assert!(
            object
                .get(&"b".into())
                .unwrap()
                .same_value(&Value::Number(9.0))
        );

        let symbol = Symbol::new(Some(JsString::from("same")));
        let other = Symbol::new(Some(JsString::from("same")));
        object
            .create_data_property(symbol.clone().into(), Value::Number(5.0))
            .unwrap();
        assert!(
            object
                .get(&symbol.into())
                .unwrap()
                .same_value(&Value::Number(5.0))
        );
        assert!(matches!(
            object.get(&other.into()).unwrap(),
            Value::Undefined
        ));
    }

    #[test]
    fn direct_array_push_preserves_complete_element_descriptors_and_guard_fallback() {
        let array = crate::runtime::ArrayObject::new(3).unwrap().object();
        assert_eq!(
            array.try_push_array_values(3, &[Value::Number(42.0)]),
            Some(4)
        );
        let descriptor = array.get_own_property(&"3".into()).unwrap();
        assert_eq!(descriptor.writable, Some(true));
        assert_eq!(descriptor.enumerable, Some(true));
        assert_eq!(descriptor.configurable, Some(true));
        assert!(descriptor.value.unwrap().same_value(&Value::Number(42.0)));
        assert_eq!(array.array_length(), 4);
        assert_eq!(array.try_push_array_values(3, &[Value::Number(99.0)]), None);
        let prototype = Object::new();
        prototype
            .create_data_property("4".into(), Value::Number(1.0))
            .unwrap();
        array.set_prototype(Some(prototype));
        assert_eq!(array.try_push_array_values(4, &[Value::Number(99.0)]), None);
        assert_eq!(array.array_length(), 4);
        assert!(array.get_own_property(&"4".into()).is_none());
    }

    #[test]
    fn array_named_data_hints_observe_length_and_skip_indexed_storage() {
        let array = crate::runtime::ArrayObject::new(3).unwrap().object();
        let key: PropertyKey = "length".into();
        let (value, index) = array.own_data_value_with_hint(&key, None).unwrap();
        assert!(value.same_value(&Value::Number(3.0)));
        array
            .create_data_property("3".into(), Value::Number(42.0))
            .unwrap();
        assert!(
            array
                .own_data_value_with_hint(&key, Some(index))
                .unwrap()
                .0
                .same_value(&Value::Number(4.0))
        );
        assert!(array.own_data_value_with_hint(&"3".into(), None).is_none());
        assert!(
            array
                .get(&"3".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
    }

    #[test]
    fn own_data_hint_rechecks_keys_descriptors_and_exotic_objects() {
        let object = Object::new();
        let prefix: PropertyKey = "prefix".into();
        let key: PropertyKey = "x".into();
        object
            .create_data_property(prefix.clone(), Value::Null)
            .unwrap();
        object
            .create_data_property(key.clone(), Value::Number(41.0))
            .unwrap();
        let (_, hint) = object.own_data_value_with_hint(&key, None).unwrap();
        assert!(object.try_delete(&prefix).unwrap());
        object
            .create_data_property("other".into(), Value::Number(99.0))
            .unwrap();
        assert!(
            object
                .own_data_value_with_hint(&key, Some(hint))
                .unwrap()
                .0
                .same_value(&Value::Number(41.0))
        );
        let getter = Object::native_function(|_, _| Ok(Value::Number(42.0)));
        assert!(
            object
                .define_own_property(
                    key.clone(),
                    PropertyDescriptor::accessor(Some(getter), None, true, true)
                )
                .unwrap()
        );
        assert!(object.own_data_value_with_hint(&key, Some(hint)).is_none());
        assert!(object.get(&key).unwrap().same_value(&Value::Number(42.0)));

        let proxy = crate::runtime::ProxyObject::new(object.clone(), Object::new()).object();
        assert!(proxy.own_data_value_with_hint(&key, None).is_none());
        assert!(proxy.get(&key).unwrap().same_value(&Value::Number(42.0)));
        let string = crate::runtime::StringObject::new("abc".into()).object();
        assert!(
            string
                .own_data_value_with_hint(&"length".into(), None)
                .is_none()
        );
        assert!(
            string
                .get(&"length".into())
                .unwrap()
                .same_value(&Value::Number(3.0))
        );
    }
}
