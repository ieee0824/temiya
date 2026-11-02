//! Property descriptors and ordinary object internal methods.

use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use super::execution::StoredRealm;
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
    pub(crate) fn integer_index(mut index: u64) -> Self {
        let mut units = [0u16; 20];
        let mut start = units.len();
        loop {
            start -= 1;
            units[start] = u16::from(b'0') + (index % 10) as u16;
            index /= 10;
            if index == 0 {
                break;
            }
        }
        JsString::from_code_units_slice(&units[start..]).into()
    }
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
    pub fn to_object(&self, context: &super::AllocationContext) -> RuntimeResult<Object> {
        self.validate()?;
        self.fill_object(Object::new_in(context))
    }

    pub(super) fn to_object_for(&self, prototype: &Object) -> RuntimeResult<Object> {
        self.validate()?;
        let object = Object::new_for(prototype);
        object.initialize_prototype(Some(prototype.clone()));
        self.fill_object(object)
    }

    pub(super) fn to_object_in(&self, context: &Object) -> RuntimeResult<Object> {
        self.validate()?;
        self.fill_object(Object::new_for(context))
    }

    fn fill_object(&self, object: Object) -> RuntimeResult<Object> {
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

pub(super) trait NativeMetadata {
    fn source(&self) -> Option<&StoredRealm>;
    fn source_mut(&mut self) -> &mut Option<StoredRealm>;
}
pub(super) trait NativeCallBody: NativeMetadata {
    fn invoke(&self, owner: &Object, this: Value, arguments: &[Value]) -> RuntimeResult<Value>;
}
pub(super) trait NativeConstructBody: NativeMetadata {
    fn invoke(&self, arguments: &[Value], new_target: Object) -> RuntimeResult<Object>;
}
type NativeCall = Rc<dyn NativeCallBody>;
type NativeConstruct = Rc<dyn NativeConstructBody>;

pub(super) struct FunctionPayload<T> {
    source: Option<StoredRealm>,
    body: T,
}
impl<T> std::ops::Deref for FunctionPayload<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.body
    }
}
impl<T> NativeMetadata for FunctionPayload<T> {
    fn source(&self) -> Option<&StoredRealm> {
        self.source.as_ref()
    }
    fn source_mut(&mut self) -> &mut Option<StoredRealm> {
        &mut self.source
    }
}
impl<F: Fn(Value, &[Value]) -> RuntimeResult<Value>> NativeCallBody for FunctionPayload<F> {
    fn invoke(&self, _: &Object, this: Value, arguments: &[Value]) -> RuntimeResult<Value> {
        (self.body)(this, arguments)
    }
}

struct ContextFunctionPayload<F> {
    source: Option<StoredRealm>,
    body: F,
}
impl<F> NativeMetadata for ContextFunctionPayload<F> {
    fn source(&self) -> Option<&StoredRealm> {
        self.source.as_ref()
    }
    fn source_mut(&mut self) -> &mut Option<StoredRealm> {
        &mut self.source
    }
}
impl<F: Fn(&super::AllocationContext, Value, &[Value]) -> RuntimeResult<Value>> NativeCallBody
    for ContextFunctionPayload<F>
{
    fn invoke(&self, owner: &Object, this: Value, arguments: &[Value]) -> RuntimeResult<Value> {
        (self.body)(owner.allocation_context(), this, arguments)
    }
}
impl<F: Fn(&[Value], Object) -> RuntimeResult<Object>> NativeConstructBody for FunctionPayload<F> {
    fn invoke(&self, arguments: &[Value], new_target: Object) -> RuntimeResult<Object> {
        (self.body)(arguments, new_target)
    }
}

pub(super) type CapturedObjectCallback =
    fn(&Object, Option<&PropertyKey>, Value, &[Value]) -> RuntimeResult<Value>;
pub(super) type LazyCapturedObjectCallback =
    fn(&Object, &StoredObject, Option<&PropertyKey>, Value, &[Value]) -> RuntimeResult<Value>;
enum CapturedObjectHandler {
    Eager(CapturedObjectCallback),
    Lazy(LazyCapturedObjectCallback),
}
pub(super) struct CapturedObjectCall {
    object: StoredObject,
    key: Option<PropertyKey>,
    callback: CapturedObjectHandler,
}

pub(super) type CapturedFactoryCallback =
    fn(Option<&Object>, Option<&PropertyKey>, Value, &[Value]) -> RuntimeResult<Value>;
pub(super) type CapturedFactoryConstructor =
    fn(&Object, Option<&PropertyKey>, &[Value], Object) -> RuntimeResult<Object>;
pub(super) struct CapturedFactory {
    object: StoredObject,
    key: Option<PropertyKey>,
    // Calls such as Number(number) need only a leaf coercion key, not a rooted prototype.
    call: Option<(bool, CapturedFactoryCallback)>,
    construct: Option<CapturedFactoryConstructor>,
    construct_prototype: Option<super::IntrinsicId>,
}

pub(super) struct BoundFunction {
    pub(super) target: StoredObject,
    this_value: StoredValue,
    arguments: Vec<StoredValue>,
    constructor: bool,
}

pub(super) enum ManagedCallable {
    ModuleBinding(super::pipeline::ModuleBinding),
    Generator(super::generator::GeneratorAction),
    Promise(super::promise::PromiseAction),
    ProxyRevocable(super::proxy::ProxyRevocable),
    ObjectFactory(super::intrinsics::ObjectFactory),
    JsonParse(super::host_data::JsonParseBuiltin),
    Date(super::host_data::DateFactory),
    Factory(CapturedFactory),
    Array(super::array_methods::ArrayBuiltin),
    CapturedObject(CapturedObjectCall),
    Bytecode(super::bytecode_function::StoredClosure),
    Ast(super::function::StoredFunctionClosure),
    FieldInitializer(super::class_fields::Initializer),
    Bound(BoundFunction),
    ProxyRevoker(std::cell::RefCell<Option<StoredObject>>),
}

impl ManagedCallable {
    pub(super) fn bound_target(&self, owner: &Object) -> RuntimeResult<Option<Object>> {
        match self {
            Self::ModuleBinding(_) => Ok(None),
            Self::Generator(_) => Ok(None),
            Self::Promise(_) => Ok(None),
            Self::ProxyRevocable(_) => Ok(None),
            Self::ObjectFactory(_) => Ok(None),
            Self::Bound(bound) => bound.target.load(owner).map(Some),
            Self::ProxyRevoker(_) => Ok(None),
            Self::Ast(_) => Ok(None),
            Self::FieldInitializer(_) => Ok(None),
            Self::Bytecode(_) => Ok(None),
            Self::CapturedObject(_) => Ok(None),
            Self::Array(_) => Ok(None),
            Self::Factory(_) => Ok(None),
            Self::Date(_) => Ok(None),
            Self::JsonParse(_) => Ok(None),
        }
    }
    fn is_callable(&self) -> bool {
        match self {
            Self::Factory(factory) => factory.call.is_some(),
            _ => true,
        }
    }
    fn is_constructor(&self) -> bool {
        match self {
            Self::ModuleBinding(_) => false,
            Self::Generator(_) => false,
            Self::Promise(_) => false,
            Self::ProxyRevocable(_) => false,
            Self::ObjectFactory(_) => true,
            Self::Bound(bound) => bound.constructor,
            Self::Ast(closure) => closure.is_constructor(),
            Self::FieldInitializer(_) => false,
            Self::Bytecode(closure) => closure.is_constructor(),
            Self::ProxyRevoker(_) => false,
            Self::CapturedObject(_) => false,
            Self::Array(_) => false,
            Self::Factory(factory) => factory.construct.is_some(),
            Self::Date(_) => true,
            Self::JsonParse(_) => false,
        }
    }
    fn call(&self, owner: &Object, this: Value, arguments: &[Value]) -> RuntimeResult<Value> {
        match self {
            Self::ModuleBinding(binding) => binding.call(owner),
            Self::Generator(action) => action.call(owner, arguments),
            Self::Promise(action) => action.call(owner, arguments),
            Self::ProxyRevocable(factory) => factory.call(owner, arguments),
            Self::ObjectFactory(factory) => factory.create(owner, arguments).map(Value::Object),
            Self::JsonParse(builtin) => builtin.call(owner, arguments),
            Self::Date(factory) => factory.call(),
            Self::Factory(factory) => {
                let (needs_object, callback) = factory
                    .call
                    .ok_or_else(|| RuntimeError::type_error("object is not callable"))?;
                let object = if needs_object {
                    Some(factory.object.load(owner)?)
                } else {
                    None
                };
                callback(object.as_ref(), factory.key.as_ref(), this, arguments)
            }
            Self::Array(call) => call.call(owner, this, arguments),
            Self::CapturedObject(call) => match call.callback {
                CapturedObjectHandler::Eager(callback) => {
                    let object = call.object.load(owner)?;
                    callback(&object, call.key.as_ref(), this, arguments)
                }
                CapturedObjectHandler::Lazy(callback) => {
                    callback(owner, &call.object, call.key.as_ref(), this, arguments)
                }
            },
            Self::Ast(closure) => closure.call(owner, this, arguments),
            Self::FieldInitializer(initializer) => initializer.call(owner, this),
            Self::Bytecode(closure) => closure.call(owner, this, arguments),
            Self::Bound(bound) => {
                let target = bound.target.load(owner)?;
                let this_value = bound.this_value.load(owner)?;
                let arguments = bound.arguments(owner, arguments)?;
                target.call(this_value, &arguments)
            }
            Self::ProxyRevoker(slot) => {
                let proxy = slot.borrow_mut().take();
                if let Some(proxy) = proxy {
                    let proxy = proxy.load(owner)?;
                    if let Some(slot) = &mut proxy.0.state.borrow_mut().proxy {
                        slot.revoke();
                    }
                }
                Ok(Value::Undefined)
            }
        }
    }
    fn construct(
        &self,
        owner: &Object,
        arguments: &[Value],
        new_target: Object,
    ) -> RuntimeResult<Object> {
        if let Self::ObjectFactory(factory) = self {
            return factory.construct(owner, arguments, new_target);
        }
        if let Self::Date(factory) = self {
            return factory.construct(owner, arguments, new_target);
        }
        if let Self::Factory(factory) = self {
            let callback = factory
                .construct
                .ok_or_else(|| RuntimeError::type_error("object is not a constructor"))?;
            let object = match factory.construct_prototype {
                // Intrinsic factories have immutable own prototype properties.
                // A distinct newTarget (including a Proxy) still performs Get.
                Some(_) if &new_target == owner => factory.object.load(owner)?,
                Some(intrinsic) => {
                    new_target.constructor_prototype(intrinsic, || factory.object.load(owner))?
                }
                None => factory.object.load(owner)?,
            };
            return callback(&object, factory.key.as_ref(), arguments, new_target);
        }
        if let Self::Ast(closure) = self {
            return closure.construct(owner, arguments, new_target);
        }
        if let Self::Bytecode(closure) = self {
            return closure.construct(owner, arguments, new_target);
        }
        let Self::Bound(bound) = self else {
            return Err(RuntimeError::type_error("object is not a constructor"));
        };
        let target = bound.target.load(owner)?;
        let arguments = bound.arguments(owner, arguments)?;
        let new_target = if new_target == *owner {
            target.clone()
        } else {
            new_target
        };
        target.construct(&arguments, new_target)
    }
    fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        match self {
            Self::ModuleBinding(binding) => binding.trace(tracer),
            Self::Generator(action) => action.trace(tracer),
            Self::Promise(action) => action.trace(tracer),
            Self::ProxyRevocable(factory) => factory.trace(tracer),
            Self::ObjectFactory(factory) => factory.trace(tracer),
            Self::Ast(closure) => closure.trace(tracer),
            Self::FieldInitializer(initializer) => initializer.trace(tracer),
            Self::Bytecode(closure) => closure.trace(tracer),
            Self::CapturedObject(call) => call.object.trace(tracer),
            Self::Array(call) => call.trace(tracer),
            Self::Factory(factory) => factory.object.trace(tracer),
            Self::Date(factory) => factory.trace(tracer),
            Self::JsonParse(builtin) => builtin.trace(tracer),
            Self::Bound(bound) => bound.trace(tracer),
            Self::ProxyRevoker(slot) => {
                if let Some(proxy) = &*slot.borrow() {
                    proxy.trace(tracer);
                }
            }
        }
    }
}

impl BoundFunction {
    fn arguments(&self, owner: &Object, suffix: &[Value]) -> RuntimeResult<Vec<Value>> {
        let mut arguments = Vec::with_capacity(self.arguments.len() + suffix.len());
        for value in &self.arguments {
            arguments.push(value.load(owner)?);
        }
        arguments.extend_from_slice(suffix);
        Ok(arguments)
    }

    fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        self.target.trace(tracer);
        self.this_value.trace(tracer);
        for argument in &self.arguments {
            argument.trace(tracer);
        }
    }
}

enum OwnGet {
    Value(Value),
    Accessor(Option<Object>),
}

#[derive(Clone)]
struct PropertyEntry {
    key: PropertyKey,
    descriptor: StoredDescriptor,
}

#[derive(Default)]
pub(super) struct ObjectState {
    pub(super) private_fields: Option<Box<super::private_fields::Fields>>,
    prototype: Option<StoredObject>,
    extensible: bool,
    properties: Vec<PropertyEntry>,
    // A position hint only. Bounds and the complete key are checked on every
    // read, so deleting/reordering properties never returns a stale descriptor.
    last_property: Cell<usize>,
    callable: Option<NativeCall>,
    pub(super) instance_fields: Vec<super::class_fields::Field>,
    pub(super) managed_callable: Option<Rc<FunctionPayload<ManagedCallable>>>,
    construct: Option<NativeConstruct>,
    // The own data property already exists. Only its initial object value is
    // deferred until an operation observes or redefines that descriptor.
    lazy_function_prototype: Option<StoredObject>,
    pub(super) array: bool,
    pub(super) elements: Option<Box<super::elements::Elements<StoredDescriptor>>>,
    pub(super) buffer: Option<super::indexed::BufferSlot>,
    pub(super) view: Option<Box<super::indexed::ViewSlot>>,
    pub(super) string_data: Option<Box<JsString>>,
    pub(super) regexp: Option<super::regexp::RegExpSlot>,
    pub(super) primitive_data: Option<Box<Value>>,
    pub(super) map_data: Option<super::collections::MapSlot>,
    pub(super) set_data: Option<super::collections::SetSlot>,
    pub(super) weak_map_data: Option<super::collections::WeakMapSlot>,
    pub(super) weak_set_data: Option<super::collections::WeakSetSlot>,
    pub(super) promise_data: Option<Rc<std::cell::RefCell<super::promise::PromiseData>>>,
    pub(super) generator_data: Option<Rc<std::cell::RefCell<super::generator::GeneratorSlot>>>,
    pub(super) module_namespace: Option<Box<super::pipeline::StoredModule>>,
    pub(super) date_value: Option<f64>,
    pub(super) iterator: Option<Box<super::iterator::IteratorSlot>>,
    pub(super) proxy: Option<Box<super::proxy::ProxySlot>>,
}

pub(super) fn ordinary_state() -> ObjectState {
    ObjectState {
        extensible: true,
        ..ObjectState::default()
    }
}

impl ObjectState {
    fn checked_property_position(&self, key: &PropertyKey) -> Option<usize> {
        let hint = self.last_property.get();
        if self
            .properties
            .get(hint)
            .is_some_and(|entry| entry.key == *key)
        {
            return Some(hint);
        }
        if self.properties.len() > 8 {
            if let Some(index) = key.array_index().map(|index| index as usize) {
                if self
                    .properties
                    .get(index)
                    .is_some_and(|entry| entry.key == *key)
                {
                    return Some(index);
                }
            }
            let last = self.properties.len() - 1;
            if self.properties[last].key == *key {
                return Some(last);
            }
        }
        self.properties.iter().position(|entry| entry.key == *key)
    }
}

impl Object {
    // Captured native next functions produce fresh, otherwise unobservable
    // result objects. Internal consumers can take their value directly, while
    // custom, bound, proxy and replaced next methods use ordinary calls.
    pub(super) fn try_builtin_iterator_step(
        &self,
        iterator: &Self,
    ) -> Option<RuntimeResult<Option<Value>>> {
        let payload = {
            let state = self.0.state.borrow();
            if state.proxy.is_some() {
                return None;
            }
            state.managed_callable.clone()?
        };
        let ManagedCallable::CapturedObject(capture) = &payload.body else {
            return None;
        };
        let CapturedObjectHandler::Eager(callback) = capture.callback else {
            return None;
        };
        let array = std::ptr::fn_addr_eq(
            callback,
            super::iterator::array_next_call as CapturedObjectCallback,
        );
        let string = std::ptr::fn_addr_eq(
            callback,
            super::iterator::string_next_call as CapturedObjectCallback,
        );
        if !array && !string {
            return None;
        }
        Some((|| {
            StoredObject::check_context(self, iterator)?;
            let _depth = CallDepthGuard::enter()?;
            let _prototype = capture.object.load(self)?;
            let result = if array {
                super::iterator::array_step_value(
                    Value::Object(iterator.clone()),
                    capture.key.as_ref().expect("iterator primitive key"),
                )
            } else {
                super::iterator::string_step_value(Value::Object(iterator.clone()))
            };
            result.map_err(|error| self.bind_native_exception(payload.source.as_ref(), error))
        })())
    }
    // Only the unpublished ordinary object created for an object literal uses
    // these initializers. No property/proxy hooks or descriptor observers can
    // see it before construction finishes.
    pub(super) fn reserve_literal_properties(&self, count: usize) {
        let mut state = self.0.state.borrow_mut();
        debug_assert!(state.properties.is_empty() && state.proxy.is_none() && !state.array);
        state.properties.reserve_exact(count.min(4));
    }

    pub(super) fn initialize_literal_accessor(
        &self,
        key: PropertyKey,
        function: Object,
        getter: bool,
    ) -> RuntimeResult<()> {
        debug_assert!(function.is_callable());
        // A key/coercion callback can collect and promote the destination.
        // Keep the checked store and old-to-young barrier even before publish.
        let function = StoredObject::store(self, &function)?;
        let mut state = self.0.state.borrow_mut();
        debug_assert!(state.proxy.is_none() && !state.array && state.extensible);
        let position = state.properties.iter().position(|entry| entry.key == key);
        let (mut get, mut set) = match position.map(|i| &state.properties[i].descriptor) {
            Some(StoredDescriptor::Accessor {
                get,
                set,
                configurable,
                ..
            }) => {
                debug_assert!(*configurable);
                (get.clone(), set.clone())
            }
            Some(StoredDescriptor::Data { configurable, .. }) => {
                debug_assert!(*configurable);
                (None, None)
            }
            None => (None, None),
        };
        if getter {
            get = Some(function);
        } else {
            set = Some(function);
        }
        let descriptor = StoredDescriptor::Accessor {
            get,
            set,
            enumerable: true,
            configurable: true,
        };
        if let Some(position) = position {
            state.properties[position].descriptor = descriptor;
        } else {
            state.properties.push(PropertyEntry { key, descriptor });
        }
        Ok(())
    }

    // Only the fresh ordinary object owned by ArrayObject initialization calls
    // this. The numeric length needs no coercion and contains no managed edge.
    pub(super) fn initialize_array(&self, length: u32) {
        let mut state = self.0.state.borrow_mut();
        debug_assert!(!state.array && state.proxy.is_none() && state.properties.is_empty());
        state.array = true;
        state.elements = Some(Box::default());
        state.properties = vec![PropertyEntry {
            key: "length".into(),
            descriptor: StoredDescriptor::Data {
                value: StoredValue::Number(f64::from(length)),
                writable: true,
                enumerable: false,
                configurable: false,
            },
        }];
    }

    // Native exceptions are unpublished ordinary objects with one own data
    // property. Prototype and message are initialized before user code can run.
    pub(super) fn native_error(prototype: Self, message: JsString) -> Self {
        let object = Self::new_with_prototype(prototype);
        object.0.state.borrow_mut().properties = vec![PropertyEntry {
            key: "message".into(),
            descriptor: StoredDescriptor::Data {
                value: StoredValue::String(message),
                writable: true,
                enumerable: false,
                configurable: true,
            },
        }];
        object
    }
    // Only freshly allocated ordinary iterator results use this constructor.
    // The incoming value remains rooted across allocation and its stored edge
    // is validated before the result becomes observable.
    pub(super) fn iterator_result(
        prototype: &Self,
        value: Value,
        done: bool,
    ) -> RuntimeResult<Self> {
        let object = Self::new_with_prototype(prototype.clone());
        let value = StoredValue::store_owned(&object, value)?;
        object.0.state.borrow_mut().properties = vec![
            PropertyEntry {
                key: "value".into(),
                descriptor: StoredDescriptor::Data {
                    value,
                    writable: true,
                    enumerable: true,
                    configurable: true,
                },
            },
            PropertyEntry {
                key: "done".into(),
                descriptor: StoredDescriptor::Data {
                    value: StoredValue::Boolean(done),
                    writable: true,
                    enumerable: true,
                    configurable: true,
                },
            },
        ];
        Ok(object)
    }
    // A new, unpublished ordinary object with unique canonical indices. The
    // public values remain rooted while their stored edges are prepared.
    pub(super) fn indexed_values_object(context: &Object, values: &[Value]) -> RuntimeResult<Self> {
        let object = Self::new_for(context);
        let mut properties = Vec::with_capacity(values.len() + 1);
        for (index, value) in values.iter().enumerate() {
            properties.push(PropertyEntry {
                key: PropertyKey::integer_index(index as u64),
                descriptor: StoredDescriptor::Data {
                    value: StoredValue::store_owned(&object, value.clone())?,
                    writable: true,
                    enumerable: true,
                    configurable: true,
                },
            });
        }
        properties.push(PropertyEntry {
            key: "length".into(),
            descriptor: StoredDescriptor::Data {
                value: StoredValue::Number(values.len() as f64),
                writable: true,
                enumerable: true,
                configurable: true,
            },
        });
        object.0.state.borrow_mut().properties = properties;
        Ok(object)
    }

    // Only ordinary data descriptors are copied; all initial managed edges
    // belong to the prototype's heap and are traced at the allocation safepoint.
    // No source borrow is held while allocation can collect.
    pub(super) fn copy_ordinary_data_rest(
        &self,
        prototype: &Self,
        excluded: &[PropertyKey],
    ) -> Option<Self> {
        let runtime = prototype.allocation_context().clone();
        if !runtime.same_heap(self.allocation_context()) {
            return None;
        }
        let properties = {
            let state = self.0.state.borrow();
            if state.proxy.is_some()
                || state.module_namespace.is_some()
                || state.array
                || state.elements.is_some()
                || state.string_data.is_some()
                || state.buffer.is_some()
                || state.view.is_some()
                || state.lazy_function_prototype.is_some()
                || state.callable.is_some()
                || state.managed_callable.is_some()
                || state.construct.is_some()
                || state.private_fields.is_some()
                || !state.instance_fields.is_empty()
                || state.regexp.is_some()
                || state.primitive_data.is_some()
                || state.map_data.is_some()
                || state.set_data.is_some()
                || state.weak_map_data.is_some()
                || state.weak_set_data.is_some()
                || state.promise_data.is_some()
                || state.generator_data.is_some()
                || state.date_value.is_some()
                || state.iterator.is_some()
                || state
                    .properties
                    .iter()
                    .any(|entry| !matches!(entry.descriptor, StoredDescriptor::Data { .. }))
            {
                return None;
            }
            let count = state
                .properties
                .iter()
                .filter(|entry| entry.descriptor.enumerable() && !excluded.contains(&entry.key))
                .count();
            let mut properties = Vec::with_capacity(count);
            for entry in &state.properties {
                let StoredDescriptor::Data {
                    value, enumerable, ..
                } = &entry.descriptor
                else {
                    unreachable!("checked data descriptors")
                };
                if *enumerable && !excluded.contains(&entry.key) {
                    properties.push(PropertyEntry {
                        key: entry.key.clone(),
                        descriptor: StoredDescriptor::Data {
                            value: value.clone(),
                            writable: true,
                            enumerable: true,
                            configurable: true,
                        },
                    });
                }
            }
            properties
        };
        let mut data = super::ObjectData::ordinary();
        let state = data.state.get_mut();
        state.prototype =
            Some(StoredObject::initial(Some(&runtime), prototype).expect("prototype context"));
        state.properties = properties;
        Some(runtime.allocate_object(data))
    }

    pub(super) fn new_with_prototype(prototype: Self) -> Self {
        let runtime = prototype.allocation_context().clone();
        let mut data = super::ObjectData::ordinary();
        data.state.get_mut().prototype = Some(
            StoredObject::initial(Some(&runtime), &prototype)
                .expect("prototype supplies the new object's allocation context"),
        );
        // The unpublished record itself protects this edge at the allocation
        // safepoint, including when this was the prototype's last public root.
        drop(prototype);
        runtime.allocate_object(data)
    }

    /// Creates a native callback that can allocate in its owner's context.
    /// The context is borrowed for each call and is not captured by the payload.
    pub fn native_function_with_context(
        context: &super::AllocationContext,
        callback: impl Fn(&super::AllocationContext, Value, &[Value]) -> RuntimeResult<Value> + 'static,
    ) -> Self {
        let object = Self::new_in(context);
        object.0.state.borrow_mut().callable = Some(Rc::new(ContextFunctionPayload {
            source: None,
            body: callback,
        }));
        object
    }
    /// Creates a Realm-independent native callback in an explicit context.
    pub fn native_function_in(
        context: &super::AllocationContext,
        callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
    ) -> Self {
        let object = Self::new_in(context);
        object.set_native_call(callback);
        object
    }

    /// Creates a native constructor in an explicit context.
    pub fn native_constructor_in(
        context: &super::AllocationContext,
        callback: impl Fn(&[Value], Object) -> RuntimeResult<Object> + 'static,
    ) -> Self {
        let object = Self::new_in(context);
        object.set_native_construct(callback);
        object
    }
    pub(super) fn source_realm(&self) -> Option<StoredRealm> {
        self.0.runtime()?;
        self.stored_source_realm()
    }

    pub(crate) fn super_base(&self) -> RuntimeResult<Value> {
        // Release the function-state borrow before following the traced home
        // object or invoking its potentially observable prototype operation.
        let callable = self.0.state.borrow().managed_callable.clone();
        match callable.as_ref().map(|callable| &callable.body) {
            Some(ManagedCallable::Ast(closure)) => closure.super_base(self),
            Some(ManagedCallable::FieldInitializer(initializer)) => initializer.super_base(self),
            _ => Err(RuntimeError::reference_error(
                "function has no super binding",
            )),
        }
    }

    fn stored_source_realm(&self) -> Option<StoredRealm> {
        let state = self.0.state.borrow();
        if let Some(call) = &state.managed_callable {
            return match &call.body {
                ManagedCallable::Ast(closure) => Some(closure.source_realm().clone()),
                ManagedCallable::Bytecode(closure) => Some(closure.source_realm().clone()),
                ManagedCallable::FieldInitializer(initializer) => {
                    Some(initializer.source_realm().clone())
                }
                _ => call.source.clone(),
            };
        }
        state
            .callable
            .as_ref()
            .and_then(|call| call.source().cloned())
            .or_else(|| {
                state
                    .construct
                    .as_ref()
                    .and_then(|body| body.source().cloned())
            })
    }

    /// Resolves the function's Realm through bound and Proxy targets. Synthetic
    /// callback source metadata on a wrapper is not an ECMAScript [[Realm]] slot.
    fn function_realm(&self) -> RuntimeResult<Option<super::Realm>> {
        let mut function = self.clone();
        loop {
            let target = {
                let state = function.0.state.borrow();
                if let Some(proxy) = &state.proxy {
                    Some(proxy.target(&function)?)
                } else if let Some(call) = &state.managed_callable {
                    call.body.bound_target(&function)?
                } else {
                    None
                }
            };
            if let Some(target) = target {
                function = target;
                continue;
            }
            return function
                .stored_source_realm()
                .map(|source| source.load(&function))
                .transpose();
        }
    }

    /// Gets a constructor's instance prototype, using its own Realm's intrinsic
    /// only after an observable getter returned a non-object value.
    pub(super) fn constructor_prototype(
        &self,
        intrinsic: super::IntrinsicId,
        fallback: impl FnOnce() -> RuntimeResult<Object>,
    ) -> RuntimeResult<Object> {
        if let Value::Object(prototype) = self.get(&"prototype".into())? {
            return Ok(prototype);
        }
        match self.function_realm()? {
            Some(realm) => Ok(realm.intrinsic(intrinsic)),
            // Embedding-created callbacks may have no associated ECMAScript Realm.
            None => fallback(),
        }
    }

    fn bind_native_exception(
        &self,
        source: Option<&StoredRealm>,
        error: RuntimeError,
    ) -> RuntimeError {
        if error.thrown_value().is_some() || error.kind() == super::RuntimeErrorKind::InternalError
        {
            return error;
        }
        match source {
            Some(source) => match source.load(self) {
                Ok(realm) => realm.bind_exception(error),
                Err(failure) => failure,
            },
            None => error,
        }
    }

    pub(super) fn initialize_native_realm(&self, realm: &super::Realm) -> RuntimeResult<()> {
        let mut state = self.0.state.borrow_mut();
        if state.callable.is_none() && state.construct.is_none() && state.managed_callable.is_none()
        {
            return Ok(());
        }
        let source = StoredRealm::store(self, realm)?;
        if let Some(body) = &mut state.callable {
            *Rc::get_mut(body)
                .ok_or_else(|| RuntimeError::internal_error("native body already active"))?
                .source_mut() = Some(source.clone());
        }
        if let Some(body) = &mut state.construct {
            *Rc::get_mut(body)
                .ok_or_else(|| RuntimeError::internal_error("constructor body already active"))?
                .source_mut() = Some(source.clone());
        }
        if let Some(body) = &mut state.managed_callable {
            Rc::get_mut(body)
                .ok_or_else(|| RuntimeError::internal_error("managed body already active"))?
                .source = Some(source);
        }
        Ok(())
    }

    pub(super) fn set_captured_factory(
        &self,
        capture: &Object,
        key: Option<PropertyKey>,
        call: Option<(bool, CapturedFactoryCallback)>,
        construct: Option<CapturedFactoryConstructor>,
    ) -> RuntimeResult<()> {
        self.set_captured_factory_with_prototype(capture, key, call, construct, None)
    }

    pub(super) fn set_captured_factory_with_prototype(
        &self,
        capture: &Object,
        key: Option<PropertyKey>,
        call: Option<(bool, CapturedFactoryCallback)>,
        construct: Option<CapturedFactoryConstructor>,
        construct_prototype: Option<super::IntrinsicId>,
    ) -> RuntimeResult<()> {
        let factory = CapturedFactory {
            object: StoredObject::store(self, capture)?,
            key,
            call,
            construct,
            construct_prototype,
        };
        self.set_managed_callable(ManagedCallable::Factory(factory));
        Ok(())
    }

    pub(super) fn set_managed_callable(&self, callable: ManagedCallable) {
        let source = self.source_realm();
        let source = if matches!(
            callable,
            ManagedCallable::Ast(_) | ManagedCallable::Bytecode(_)
        ) {
            None
        } else {
            source
        };
        let callable = FunctionPayload {
            source,
            body: callable,
        };
        let retired = {
            let mut state = self.0.state.borrow_mut();
            (
                state.callable.take(),
                state.construct.take(),
                state.managed_callable.replace(Rc::new(callable)),
            )
        };
        drop(retired);
    }
    pub(super) fn new_bound(
        target: &Object,
        this_value: &Value,
        arguments: &[Value],
    ) -> RuntimeResult<Self> {
        let object = Self::new_for(target);
        let bound = BoundFunction {
            target: StoredObject::store(&object, target)?,
            this_value: StoredValue::store(&object, this_value)?,
            arguments: arguments
                .iter()
                .map(|value| StoredValue::store(&object, value))
                .collect::<RuntimeResult<_>>()?,
            constructor: target.is_constructor(),
        };
        object.set_managed_callable(ManagedCallable::Bound(bound));
        Ok(object)
    }
    #[cfg(test)]
    pub(super) fn initialize_proxy_revoker(&self, proxy: &Object) -> RuntimeResult<()> {
        let proxy = StoredObject::store(self, proxy)?;
        self.set_managed_callable(ManagedCallable::ProxyRevoker(std::cell::RefCell::new(
            Some(proxy),
        )));
        Ok(())
    }
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
        state.elements.as_ref()?.get_value(index, self)
    }

    // A hint identifies a position, never a cached Value or assumed shape.
    // Validate the current key/descriptor under one borrow before returning.
    pub(crate) fn own_data_value_with_hint(
        &self,
        key: &PropertyKey,
        hint: Option<usize>,
    ) -> Option<(Value, usize)> {
        let state = self.0.state.borrow();
        // String length and canonical indices are virtual properties; named
        // ordinary data properties can use the same checked lookup as objects.
        if state.proxy.is_some()
            || state.lazy_function_prototype.is_some()
            || (state.string_data.is_some()
                && (matches!(key, PropertyKey::String(name) if name.code_units().iter().copied().eq("length".encode_utf16()))
                    || key.array_index().is_some()))
            || (state.elements.is_some() && key.array_index().is_some())
            || state.view.is_some()
            || state.buffer.is_some()
        {
            return None;
        }
        let (index, entry) = hint
            .or(Some(state.last_property.get()))
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
        let value = entry.descriptor.value()?.load(self).ok()?;
        state.last_property.set(index);
        Some((value, index))
    }

    // Only for a freshly allocated function before publication. Length is a
    // leaf value and its standard descriptor is known.
    pub(super) fn initialize_function_length(&self, length: usize, constructable: bool) {
        let mut state = self.0.state.borrow_mut();
        debug_assert!(state.properties.is_empty());
        debug_assert!(state.proxy.is_none() && !state.array);
        // Named/inferred functions may add name and prototype. Non-constructable
        // functions need at most their inferred name after this length entry.
        state.properties.reserve(if constructable { 3 } else { 2 });
        state.properties.push(PropertyEntry {
            key: "length".into(),
            descriptor: StoredDescriptor::Data {
                value: StoredValue::Number(length as f64),
                writable: false,
                enumerable: false,
                configurable: true,
            },
        });
    }

    // Class functions initialize both standard leaves before publication.
    pub(super) fn initialize_function_properties(
        &self,
        length: usize,
        name: JsString,
        object_prototype: Option<Object>,
    ) -> RuntimeResult<()> {
        let object_prototype = object_prototype
            .as_ref()
            .map(|parent| StoredObject::store(self, parent))
            .transpose()?;
        let mut state = self.0.state.borrow_mut();
        debug_assert!(state.properties.is_empty());
        debug_assert!(state.proxy.is_none() && !state.array);
        // Leave room for the default constructable function's prototype entry.
        state.properties.reserve(3);
        for (key, value) in [
            ("length", StoredValue::Number(length as f64)),
            ("name", StoredValue::String(name)),
        ] {
            state.properties.push(PropertyEntry {
                key: key.into(),
                descriptor: StoredDescriptor::Data {
                    value,
                    writable: false,
                    enumerable: false,
                    configurable: true,
                },
            });
        }
        if let Some(parent) = object_prototype {
            state.properties.push(PropertyEntry {
                key: "prototype".into(),
                descriptor: StoredDescriptor::Data {
                    value: StoredValue::Undefined,
                    writable: true,
                    enumerable: false,
                    configurable: false,
                },
            });
            state.lazy_function_prototype = Some(parent);
        }
        Ok(())
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

    fn legacy_property_key(key: &PropertyKey) -> bool {
        matches!(key, PropertyKey::String(name) if name.code_units() == [99, 97, 108, 108, 101, 114] || name.code_units() == [97, 114, 103, 117, 109, 101, 110, 116, 115])
    }

    fn materialize_legacy_function_properties(&self) {
        let mut state = self.0.state.borrow_mut();
        let legacy = state
            .managed_callable
            .as_ref()
            .is_some_and(|call| match &call.body {
                ManagedCallable::Ast(closure) => closure.has_legacy_properties(),
                ManagedCallable::Bytecode(closure) => closure.has_legacy_properties(),
                _ => false,
            });
        if !legacy {
            return;
        }
        // A redacted legacy extension: never expose a caller or an arguments
        // frame. Defer the properties until reflection/access to avoid extra
        // allocations for ordinary function creation and calls.
        for (index, name) in ["caller", "arguments"].into_iter().enumerate() {
            let key = PropertyKey::from(name);
            if !state.properties.iter().any(|entry| entry.key == key) {
                state.properties.insert(
                    index,
                    PropertyEntry {
                        key,
                        descriptor: StoredDescriptor::Data {
                            value: StoredValue::Null,
                            writable: false,
                            enumerable: false,
                            configurable: false,
                        },
                    },
                );
            }
        }
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
        let prototype = Object::new_in(self.allocation_context());
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
        let source = self.source_realm();
        let retired = self
            .0
            .state
            .borrow_mut()
            .callable
            .replace(Rc::new(FunctionPayload {
                source,
                body: callback,
            }));
        drop(retired);
    }

    pub(crate) fn set_native_construct(
        &self,
        callback: impl Fn(&[Value], Object) -> RuntimeResult<Object> + 'static,
    ) {
        let source = self.source_realm();
        let retired = self
            .0
            .state
            .borrow_mut()
            .construct
            .replace(Rc::new(FunctionPayload {
                source,
                body: callback,
            }));
        drop(retired);
    }

    pub(crate) fn clear_native_construct(&self) {
        let retired = self.0.state.borrow_mut().construct.take();
        drop(retired);
    }

    pub(crate) fn clear_native_call(&self) {
        let retired = self.0.state.borrow_mut().callable.take();
        drop(retired);
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

    pub(super) fn native_function_with_metadata(
        prototype: &Object,
        name: &str,
        length: u32,
        callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
    ) -> Self {
        Self::function_with_metadata(
            prototype,
            name,
            length,
            Some(Rc::new(FunctionPayload {
                source: None,
                body: callback,
            })),
            None,
        )
    }

    pub(super) fn native_function_with_object_capture(
        prototype: &Object,
        name: &str,
        length: u32,
        capture: &Object,
        key: Option<PropertyKey>,
        callback: CapturedObjectCallback,
    ) -> RuntimeResult<Self> {
        Self::captured_function_with_metadata(
            prototype,
            name,
            length,
            capture,
            key,
            CapturedObjectHandler::Eager(callback),
        )
    }

    pub(super) fn native_function_with_lazy_object_capture(
        prototype: &Object,
        name: &str,
        length: u32,
        capture: &Object,
        key: Option<PropertyKey>,
        callback: LazyCapturedObjectCallback,
    ) -> RuntimeResult<Self> {
        Self::captured_function_with_metadata(
            prototype,
            name,
            length,
            capture,
            key,
            CapturedObjectHandler::Lazy(callback),
        )
    }

    fn captured_function_with_metadata(
        prototype: &Object,
        name: &str,
        length: u32,
        capture: &Object,
        key: Option<PropertyKey>,
        callback: CapturedObjectHandler,
    ) -> RuntimeResult<Self> {
        let call = CapturedObjectCall {
            object: StoredObject::initial(prototype.0.runtime(), capture)?,
            key,
            callback,
        };
        Ok(Self::function_with_metadata(
            prototype,
            name,
            length,
            None,
            Some(ManagedCallable::CapturedObject(call)),
        ))
    }

    pub(super) fn managed_function_with_metadata(
        prototype: &Object,
        name: &str,
        length: u32,
        callable: ManagedCallable,
    ) -> Self {
        Self::function_with_metadata(prototype, name, length, None, Some(callable))
    }

    fn function_with_metadata(
        prototype: &Object,
        name: &str,
        length: u32,
        mut callable: Option<NativeCall>,
        managed: Option<ManagedCallable>,
    ) -> Self {
        // This fresh ordinary object has no existing descriptors, exotic hooks,
        // or observable identity. Seed complete properties in insertion order
        // without descriptor validation or a Vec growth allocation for four
        // entries when only two are needed. Later mutations use normal methods.
        let object = Self::new_for(prototype);
        let source = prototype.source_realm();
        if let Some(source) = &source {
            source.record_owner(&object).expect("native source context");
        }
        if let Some(callback) = &mut callable {
            *Rc::get_mut(callback)
                .expect("fresh native body")
                .source_mut() = source.clone();
        }
        let managed = managed.map(|body| Rc::new(FunctionPayload { source, body }));
        {
            let mut state = object.0.state.borrow_mut();
            state.callable = callable;
            state.managed_callable = managed;
            state.prototype =
                Some(StoredObject::store(&object, prototype).expect("native prototype context"));
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

    /// Allocates an object with both native `[[Call]]` and `[[Construct]]` methods.
    #[must_use]
    pub fn native_callable_constructor(
        context: &super::AllocationContext,
        call: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
        construct: impl Fn(&[Value], Object) -> RuntimeResult<Object> + 'static,
    ) -> Self {
        let object = Self::native_function_in(context, call);
        object.set_native_construct(construct);
        object
    }

    /// Returns whether this object has a `[[Call]]` internal method.
    #[must_use]
    pub fn is_callable(&self) -> bool {
        let state = self.0.state.borrow();
        state.callable.is_some()
            || state
                .managed_callable
                .as_ref()
                .is_some_and(|call| call.is_callable())
            || state.proxy.as_ref().is_some_and(|proxy| proxy.callable)
    }

    // Same-Realm shortcut for the immutable inherited default method only.
    pub(super) fn try_default_instance_of(
        &self,
        realm: &super::Realm,
        left: &Value,
        key: &PropertyKey,
    ) -> Option<RuntimeResult<bool>> {
        let function_prototype = realm.intrinsic(super::IntrinsicId::FunctionPrototype);
        {
            let state = self.0.state.borrow();
            // Function.prototype owns the installed, nonwritable and
            // nonconfigurable @@hasInstance. Only direct ordinary inheritance
            // with no own override can bypass GetMethod and native dispatch.
            if state.proxy.is_some()
                || state.prototype.as_ref()?.0 != function_prototype.0.handle()?
                || state.properties.iter().any(|entry| entry.key == *key)
                || state
                    .managed_callable
                    .as_ref()
                    .is_some_and(|call| matches!(call.body, ManagedCallable::Bound(_)))
                || !(state.callable.is_some()
                    || state
                        .managed_callable
                        .as_ref()
                        .is_some_and(|call| call.is_callable()))
            {
                return None;
            }
        }
        Some((|| {
            // Preserve the skipped native call's context checks and depth
            // budget, including when a prototype trap reenters the engine.
            StoredObject::check_context(&function_prototype, self)?;
            if let Value::Object(object) = left {
                StoredObject::check_context(&function_prototype, object)?;
            }
            let _depth = CallDepthGuard::enter()?;
            let result = super::operators::ordinary_has_instance(
                Value::Object(self.clone()),
                left.clone(),
                key,
            )
            .map_err(|error| realm.bind_exception(error));
            if let Err(error) = &result {
                self.check_abrupt_context(error)?;
            }
            result
        })())
    }

    /// Invokes this object's `[[Call]]` internal method.
    ///
    /// Calls and construction share a per-thread nesting limit of 32. Exceeding
    /// the limit returns RangeError; normal and abrupt exits release the budget.
    pub fn call(&self, this_value: Value, arguments: &[Value]) -> RuntimeResult<Value> {
        let (callback, bound, proxy) = {
            let state = self.0.state.borrow();
            (
                state.callable.clone(),
                state
                    .managed_callable
                    .as_ref()
                    .filter(|call| call.is_callable())
                    .cloned(),
                state.proxy.as_ref().is_some_and(|proxy| proxy.callable),
            )
        };
        if callback.is_none() && bound.is_none() && !proxy {
            return Err(RuntimeError::type_error(
                "object does not have a [[Call]] internal method",
            ));
        }
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
        let result = if proxy {
            super::proxy::apply_slot(self, this_value, arguments)
        } else if let Some(bound) = &bound {
            bound.call(self, this_value, arguments)
        } else {
            callback
                .as_ref()
                .expect("validated callable")
                .invoke(self, this_value, arguments)
        };
        let result = result.map_err(|error| {
            self.bind_native_exception(
                bound
                    .as_ref()
                    .and_then(|body| body.source.as_ref())
                    .or_else(|| callback.as_ref().and_then(|body| body.source())),
                error,
            )
        });
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
        let state = self.0.state.borrow();
        state.construct.is_some()
            || state
                .managed_callable
                .as_ref()
                .is_some_and(|bound| bound.is_constructor())
            || state.proxy.as_ref().is_some_and(|proxy| proxy.constructor)
    }

    /// Invokes this object's `[[Construct]]` internal method.
    /// Shares the nesting limit documented on [`Self::call`].
    pub fn construct(&self, arguments: &[Value], new_target: Self) -> RuntimeResult<Self> {
        let (callback, bound, proxy) = {
            let state = self.0.state.borrow();
            (
                state.construct.clone(),
                state.managed_callable.clone(),
                state.proxy.as_ref().is_some_and(|proxy| proxy.constructor),
            )
        };
        if callback.is_none()
            && !bound.as_ref().is_some_and(|bound| bound.is_constructor())
            && !proxy
        {
            return Err(RuntimeError::type_error(
                "object does not have a [[Construct]] internal method",
            ));
        }
        if self.0.runtime().is_some() {
            StoredObject::check_context(self, &new_target)?;
            for value in arguments {
                if let Value::Object(value) = value {
                    StoredObject::check_context(self, value)?;
                }
            }
        }
        let _depth = CallDepthGuard::enter()?;
        let result = if proxy {
            super::proxy::construct_slot(self, arguments, new_target)
        } else if let Some(bound) = &bound {
            bound.construct(self, arguments, new_target)
        } else {
            callback
                .as_ref()
                .expect("validated constructor")
                .invoke(arguments, new_target)
        };
        let result = result.map_err(|error| {
            // These bodies bind errors after entering their own Realm. Instance
            // allocation precedes that entry; Bound/Proxy forwarding also keeps
            // the caller context. Do not rebind an unmaterialized allocation error.
            if proxy
                || bound.as_ref().is_some_and(|call| {
                    matches!(
                        &call.body,
                        ManagedCallable::Ast(_)
                            | ManagedCallable::Bytecode(_)
                            | ManagedCallable::Bound(_)
                    )
                })
            {
                return error;
            }
            self.bind_native_exception(
                bound
                    .as_ref()
                    .and_then(|body| body.source.as_ref())
                    .or_else(|| callback.as_ref().and_then(|body| body.source())),
                error,
            )
        });
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
        if state.module_namespace.is_some() {
            // Export getters are internal storage for live bindings. Expose
            // the namespace's data descriptor and propagate an export's TDZ
            // error rather than treating the binding as an accessor property.
            if let Some(entry) = state.properties.iter().find(|entry| entry.key == *key) {
                let descriptor = entry.descriptor.load(self)?;
                if let Some(getter) = descriptor.get.flatten() {
                    drop(state);
                    let value = getter.call(Value::Object(self.clone()), &[])?;
                    return Ok(Some(PropertyDescriptor::data(value, true, true, false)));
                }
            }
        }
        if Self::legacy_property_key(key) {
            drop(state);
            self.materialize_legacy_function_properties();
            state = self.0.state.borrow();
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
        if Self::legacy_property_key(&key) {
            self.materialize_legacy_function_properties();
        }
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

    pub(super) fn ordinary_array_length_info(&self) -> Option<(u32, bool)> {
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
        if let Value::Object(value) = &value {
            StoredObject::check_context(self, value)?;
        }
        {
            let mut state = self.0.state.borrow_mut();
            if state.array
                && state.proxy.is_none()
                && state.string_data.is_none()
                && state.view.is_none()
                && state.buffer.is_none()
                && state.elements.is_some()
            {
                if let Some(index) = key.array_index() {
                    let length = state
                        .properties
                        .iter()
                        .enumerate()
                        .find_map(|(slot, entry)| {
                            if entry.key != PropertyKey::from("length") {
                                return None;
                            }
                            let Some(StoredValue::Number(length)) = entry.descriptor.value() else {
                                return None;
                            };
                            Some((
                                slot,
                                *length as u32,
                                entry.descriptor.writable() == Some(true),
                            ))
                        });
                    if let Some((length_slot, length, writable)) = length {
                        if index >= length && !writable {
                            return Ok(false);
                        }
                        let current = state.elements.as_ref().unwrap().get(index);
                        let exists = current.is_some();
                        if current.is_some_and(|entry| entry.configurable() != Some(true))
                            || (!exists && !state.extensible)
                        {
                            return Ok(false);
                        }
                        // CreateDataProperty supplies every data attribute. No
                        // descriptor load or prototype operation is needed, but
                        // the incoming managed edge must precede publication.
                        let value = StoredValue::store_owned(self, value)?;
                        state.elements.as_mut().unwrap().insert(
                            index,
                            StoredDescriptor::Data {
                                value,
                                writable: true,
                                enumerable: true,
                                configurable: true,
                            },
                        );
                        if index >= length {
                            state.properties[length_slot]
                                .descriptor
                                .set_value(StoredValue::Number(f64::from(index + 1)));
                        }
                        return Ok(true);
                    }
                }
            }
            if state.proxy.is_none()
                && !state.array
                && state.string_data.is_none()
                && state.lazy_function_prototype.is_none()
            {
                let position = state.properties.iter().position(|entry| entry.key == key);
                if let Some(position) = position {
                    // CreateDataProperty requests configurable=true, so an
                    // existing nonconfigurable property always rejects it.
                    if state.properties[position].descriptor.configurable() != Some(true) {
                        return Ok(false);
                    }
                } else if !state.extensible {
                    return Ok(false);
                }
                // All attributes are supplied and true. A configurable own
                // descriptor can be replaced without loading its old value or
                // getter facades. Validate/store the new edge before publishing.
                let descriptor = StoredDescriptor::Data {
                    value: StoredValue::store_owned(self, value)?,
                    writable: true,
                    enumerable: true,
                    configurable: true,
                };
                if let Some(position) = position {
                    state.properties[position].descriptor = descriptor;
                } else {
                    state.properties.push(PropertyEntry { key, descriptor });
                }
                return Ok(true);
            }
        }
        self.define_own_property(key, PropertyDescriptor::data(value, true, true, true))
    }

    /// Implements `HasProperty`, including the prototype chain.
    pub fn has_property(&self, key: &PropertyKey) -> RuntimeResult<bool> {
        {
            let state = self.0.state.borrow();
            if state.proxy.is_some() {
                drop(state);
                return super::proxy::has(self, key);
            }
            if state.module_namespace.is_some() && matches!(key, PropertyKey::String(_)) {
                // Namespace [[HasProperty]] tests the export names, without
                // GetBindingValue. An uninitialized export still exists; only
                // Get and GetOwnProperty consume its value and observe TDZ.
                return Ok(state.properties.iter().any(|entry| entry.key == *key));
            }
        }
        if self.get_own_property(key).is_some() {
            return Ok(true);
        }
        self.prototype()
            .map_or(Ok(false), |prototype| prototype.has_property(key))
    }

    // CopyDataProperties needs the own descriptor before Get. An ordinary
    // property has no callback between selecting its descriptor and Get. Load
    // the value/getter once; release the borrow before calling an accessor.
    // None means the caller must retain the full exotic/legacy path;
    // Some(Ok(None)) means the property is absent or non-enumerable.
    pub(super) fn try_own_enumerable_value(
        &self,
        key: &PropertyKey,
    ) -> Option<RuntimeResult<Option<Value>>> {
        let state = self.0.state.borrow();
        if state.proxy.is_some()
            || state.module_namespace.is_some()
            || state.lazy_function_prototype.is_some()
            || state.array
            || state.string_data.is_some()
            || state.view.is_some()
            || state.buffer.is_some()
        {
            return None;
        }
        let Some(index) = state.checked_property_position(key) else {
            return Some(Ok(None));
        };
        match &state.properties[index].descriptor {
            StoredDescriptor::Data {
                value, enumerable, ..
            } => {
                state.last_property.set(index);
                Some(if *enumerable {
                    value.load(self).map(Some)
                } else {
                    Ok(None)
                })
            }
            StoredDescriptor::Accessor {
                get, enumerable, ..
            } => {
                if Self::legacy_property_key(key) {
                    return None;
                }
                if !enumerable {
                    return Some(Ok(None));
                }
                let getter = get.as_ref().map(|getter| getter.load(self)).transpose();
                drop(state);
                Some(
                    getter
                        .and_then(|getter| {
                            getter.map_or(Ok(Value::Undefined), |getter| {
                                getter.call(Value::Object(self.clone()), &[])
                            })
                        })
                        .map(Some),
                )
            }
        }
    }

    // Select the needed edge under one ordinary-property borrow. Root the
    // current value/getter before releasing it; no user code runs under borrow.
    fn own_get_value(&self, key: &PropertyKey) -> RuntimeResult<Option<OwnGet>> {
        let state = self.0.state.borrow();
        if state.proxy.is_some()
            || state.module_namespace.is_some()
            || state.lazy_function_prototype.is_some()
            || (state.string_data.is_some()
                && (matches!(key, PropertyKey::String(name) if name.code_units().iter().copied().eq("length".encode_utf16()))
                    || key.array_index().is_some()))
            || (state.elements.is_some() && key.array_index().is_some())
            || state.view.is_some()
            || state.buffer.is_some()
        {
            return Ok(None);
        }
        let entry = state
            .checked_property_position(key)
            .map(|index| (index, &state.properties[index]));
        let Some((index, entry)) = entry else {
            return Ok(None);
        };
        let value = match &entry.descriptor {
            StoredDescriptor::Data { value, .. } => OwnGet::Value(value.load(self)?),
            StoredDescriptor::Accessor { get, .. } => {
                if Self::legacy_property_key(key) {
                    return Ok(None);
                }
                OwnGet::Accessor(get.as_ref().map(|method| method.load(self)).transpose()?)
            }
        };
        state.last_property.set(index);
        Ok(Some(value))
    }

    fn own_accessor_setter(&self, key: &PropertyKey) -> RuntimeResult<Option<Option<Object>>> {
        if Self::legacy_property_key(key) {
            return Ok(None);
        }
        let state = self.0.state.borrow();
        if state.proxy.is_some()
            || state.module_namespace.is_some()
            || state.lazy_function_prototype.is_some()
            || state.array
            || state.string_data.is_some()
            || state.view.is_some()
            || state.buffer.is_some()
        {
            return Ok(None);
        }
        let entry = state
            .properties
            .get(state.last_property.get())
            .filter(|entry| entry.key == *key)
            .or_else(|| state.properties.iter().find(|entry| entry.key == *key));
        let Some(PropertyEntry {
            descriptor: StoredDescriptor::Accessor { set, .. },
            ..
        }) = entry
        else {
            return Ok(None);
        };
        Ok(Some(
            set.as_ref().map(|method| method.load(self)).transpose()?,
        ))
    }

    /// Implements ordinary `[[Get]]` with this object as the receiver.
    pub fn get(&self, key: &PropertyKey) -> RuntimeResult<Value> {
        self.get_with_receiver(key, Value::Object(self.clone()))
    }

    /// Implements ordinary `[[Get]]` with an explicit receiver.
    pub fn get_with_receiver(&self, key: &PropertyKey, receiver: Value) -> RuntimeResult<Value> {
        if let Some(property) = self.own_get_value(key)? {
            return match property {
                OwnGet::Value(value) => Ok(value),
                OwnGet::Accessor(getter) => {
                    getter.map_or(Ok(Value::Undefined), |getter| getter.call(receiver, &[]))
                }
            };
        }
        if self.0.state.borrow().proxy.is_some() {
            return super::proxy::get(self, key, receiver);
        }
        let Some(descriptor) = self.try_get_own_property(key)? else {
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
        if self.0.state.borrow().module_namespace.is_some() {
            return Ok(false);
        }
        // An own writable data property shadows every prototype. With the same
        // ordinary receiver, Set changes only its value; no descriptor needs to
        // be loaded into rooted facades and then rebuilt by DefineOwnProperty.
        let mut ordinary_accessor = true;
        if matches!(&receiver, Value::Object(receiver) if self.0.same_object(&receiver.0)) {
            let mut state = self.0.state.borrow_mut();
            ordinary_accessor = !state.array
                && state.lazy_function_prototype.is_none()
                && state.string_data.is_none()
                && state.view.is_none()
                && state.buffer.is_none();
            if ordinary_accessor {
                if let Some(index) = state.properties.iter().position(|entry| entry.key == *key) {
                    if state.properties[index].descriptor.writable() == Some(true) {
                        // Check heap ownership and record the strong edge before
                        // changing the value. This performs no JS call or GC.
                        let stored = StoredValue::store(self, &value)?;
                        state.properties[index].descriptor.set_value(stored);
                        state.last_property.set(index);
                        return Ok(true);
                    }
                }
            }
        }
        if let Some(setter) = if ordinary_accessor {
            self.own_accessor_setter(key)?
        } else {
            None
        } {
            let Some(setter) = setter else {
                return Ok(false);
            };
            setter.call(receiver, &[value])?;
            return Ok(true);
        }
        let descriptor = self.try_get_own_property(key)?;
        let descriptor = if let Some(descriptor) = descriptor {
            descriptor
        } else {
            if let Some(prototype) = self.prototype() {
                return prototype.set_with_receiver(key, value, receiver);
            }
            PropertyDescriptor::data(Value::Undefined, true, true, true)
        };
        if descriptor.is_data_descriptor() {
            if descriptor.writable != Some(true) {
                return Ok(false);
            }
            let Value::Object(receiver) = receiver else {
                return Ok(false);
            };
            if let Some(existing) = receiver.try_get_own_property(key)? {
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
        if Self::legacy_property_key(key) {
            self.materialize_legacy_function_properties();
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

    /// Reads ordinary enumerable key flags without rooting property values.
    pub(super) fn enumerable_own_string_keys(&self) -> RuntimeResult<Vec<JsString>> {
        let state = self.0.state.borrow();
        if state.proxy.is_none() && state.string_data.is_none() && state.elements.is_none() {
            let mut indices = Vec::new();
            let mut strings = Vec::new();
            for entry in &state.properties {
                if !entry.descriptor.enumerable() {
                    continue;
                }
                let PropertyKey::String(name) = &entry.key else {
                    continue;
                };
                if let Some(index) = entry.key.array_index() {
                    indices.push((index, name.clone()));
                } else {
                    strings.push(name.clone());
                }
            }
            indices.sort_unstable_by_key(|(index, _)| *index);
            let mut result: Vec<_> = indices.into_iter().map(|(_, name)| name).collect();
            result.extend(strings);
            return Ok(result);
        }
        drop(state);
        let mut result = Vec::new();
        for key in self.try_own_property_keys()? {
            if !matches!(key, PropertyKey::String(_)) {
                continue;
            }
            if self
                .try_get_own_property(&key)?
                .is_some_and(|descriptor| descriptor.enumerable == Some(true))
            {
                let PropertyKey::String(name) = key else {
                    unreachable!()
                };
                result.push(name);
            }
        }
        Ok(result)
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
        self.materialize_legacy_function_properties();
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
        if let Self::Number(number) = self {
            if *number >= 0.0 && *number <= f64::from(u32::MAX) && number.fract() == 0.0 {
                return Ok(PropertyKey::integer_index(*number as u64));
            }
        }
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
        if let Some(fields) = &state.private_fields {
            for (_, value) in fields.iter() {
                value.trace(tracer);
            }
        }
        for field in &state.instance_fields {
            field.trace(tracer);
        }
        if let Some(module) = &state.module_namespace {
            module.trace(tracer);
        }
        if let Some(generator) = &state.generator_data {
            generator.borrow().trace(tracer);
        }
        if let Some(promise) = &state.promise_data {
            promise.borrow().trace(tracer);
        }
        if let Some(prototype) = &state.prototype {
            prototype.trace(tracer);
        }
        if let Some(parent) = &state.lazy_function_prototype {
            parent.trace(tracer);
        }
        if let Some(bound) = &state.managed_callable {
            bound.trace(tracer);
            if let Some(source) = &bound.source {
                source.trace(tracer);
            }
        }
        for source in [
            state.callable.as_ref().and_then(|body| body.source()),
            state.construct.as_ref().and_then(|body| body.source()),
        ]
        .into_iter()
        .flatten()
        {
            source.trace(tracer);
        }
        if let Some(iterator) = &state.iterator {
            iterator.trace(tracer);
        }
        if let Some(proxy) = &state.proxy {
            proxy.trace(tracer);
        }
        if let Some(entries) = &state.map_data {
            for (key, value) in entries.borrow().iter().flatten() {
                key.trace(tracer);
                value.trace(tracer);
            }
        }
        if let Some(entries) = &state.set_data {
            for value in entries.borrow().iter().flatten() {
                value.trace(tracer);
            }
        }
        if let Some(entries) = &state.weak_map_data {
            for (key, value) in entries.borrow().iter() {
                if let (Some(key), StoredValue::Object(StoredObject(value))) =
                    (key.0.handle(), value)
                {
                    tracer.ephemeron(key, *value);
                }
            }
        }
        for entry in &state.properties {
            entry.descriptor.trace(tracer);
        }
        if let Some(elements) = &state.elements {
            elements.for_each_traceable(|descriptor| descriptor.trace(tracer));
        }
    }
}

#[cfg(test)]
mod own_data_hint_tests {
    use super::*;

    #[cfg(not(miri))]
    #[test]
    fn accessor_operations_only_revive_the_invoked_method() {
        let runtime = crate::runtime::Runtime::new();
        let object = Object::new_in(&runtime);
        let getter = Object::native_function_in(&runtime, |_, _| Ok(Value::Number(42.0)));
        let setter = Object::native_function_in(&runtime, |_, args| {
            assert!(args[0].same_value(&Value::Number(42.0)));
            Ok(Value::Undefined)
        });
        object
            .define_own_property(
                "value".into(),
                PropertyDescriptor::accessor(Some(getter), Some(setter), true, true),
            )
            .unwrap();
        runtime.collect_garbage().unwrap();
        let (_, before, _) = runtime.profile_snapshot();
        for _ in 0..100 {
            assert!(
                object
                    .get(&"value".into())
                    .unwrap()
                    .same_value(&Value::Number(42.0))
            );
            assert!(object.set(&"value".into(), Value::Number(42.0)).unwrap());
        }
        let (_, after, _) = runtime.profile_snapshot();
        assert_eq!(
            after[0] - before[0],
            200,
            "Get must not revive setter, Set must not revive getter"
        );
        assert_eq!(after[1] - before[1], 200);
        drop(object);
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
    }

    #[test]
    fn descriptor_lookup_hint_rechecks_keys_and_reads_current_descriptors() {
        let context = crate::runtime::AllocationContext::new();
        let object = Object::new_in(&context);
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

        let getter = Object::native_function_in(&context, |_, _| Ok(Value::Number(9.0)));
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
        let context = crate::runtime::AllocationContext::new();
        let array = crate::runtime::ArrayObject::new(&context, 3)
            .unwrap()
            .object();
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
        let prototype = Object::new_in(&context);
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
        let context = crate::runtime::AllocationContext::new();
        let array = crate::runtime::ArrayObject::new(&context, 3)
            .unwrap()
            .object();
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
        let context = crate::runtime::AllocationContext::new();
        let object = Object::new_in(&context);
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
        let getter = Object::native_function_in(&context, |_, _| Ok(Value::Number(42.0)));
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

        let proxy =
            crate::runtime::ProxyObject::new(object.clone(), Object::new_in(&context)).object();
        assert!(proxy.own_data_value_with_hint(&key, None).is_none());
        assert!(proxy.get(&key).unwrap().same_value(&Value::Number(42.0)));
        let string = crate::runtime::StringObject::new(&context, "abc".into()).object();
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

#[cfg(test)]
mod ordinary_rest_copy_tests {
    use super::*;
    #[test]
    fn incoming_rest_record_traces_copied_edges_after_source_release() {
        let context = super::super::AllocationContext::new();
        context.set_allocation_collection_interval(1);
        let source = Object::new_in(&context);
        let child = Object::new_in(&context);
        child
            .create_data_property("answer".into(), Value::Number(42.0))
            .unwrap();
        let weak = child.downgrade();
        source
            .create_data_property("value".into(), Value::Object(child))
            .unwrap();
        let prototype = Object::new_in(&context);
        let weak_prototype = prototype.downgrade();
        let rest = source.copy_ordinary_data_rest(&prototype, &[]).unwrap();
        drop(source);
        drop(prototype);
        context.collect_garbage().unwrap();
        assert!(weak.upgrade().is_some());
        assert_eq!(rest.prototype(), weak_prototype.upgrade());
        let Value::Object(value) = rest.get(&"value".into()).unwrap() else {
            panic!("copied object")
        };
        assert!(
            value
                .get(&"answer".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop(value);
        context.verify_migration_barriers().unwrap();
        drop(rest);
        assert_eq!(context.collect_garbage().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }
}

#[cfg(test)]
mod default_instance_tests {
    use super::*;
    use crate::runtime::{
        AllocationContext, IntrinsicId, Realm, RuntimeErrorKind, WellKnownSymbol,
    };

    #[test]
    fn default_instance_shortcut_keeps_overrides_context_and_call_budget() {
        let context = AllocationContext::new();
        let realm = Realm::ordinary_in(&context);
        let constructor = realm.intrinsic(IntrinsicId::TypeError);
        let key = realm.well_known_symbol(WellKnownSymbol::HasInstance).into();
        let value = Value::Object(Object::native_error(
            realm.intrinsic(IntrinsicId::TypeErrorPrototype),
            "probe".into(),
        ));
        assert!(
            constructor
                .try_default_instance_of(&realm, &value, &key)
                .unwrap()
                .unwrap()
        );
        let Value::Object(method) = realm
            .intrinsic(IntrinsicId::FunctionPrototype)
            .get(&key)
            .unwrap()
        else {
            panic!("builtin")
        };
        let foreign = Value::Object(Object::new_in(&AllocationContext::new()));
        let direct = method
            .call(
                Value::Object(constructor.clone()),
                std::slice::from_ref(&foreign),
            )
            .unwrap_err();
        let shortcut = constructor
            .try_default_instance_of(&realm, &foreign, &key)
            .unwrap()
            .unwrap_err();
        assert_eq!(direct.kind(), shortcut.kind());
        assert!(direct.thrown_value().is_none() && shortcut.thrown_value().is_none());
        let guards = (0..MAX_CALL_DEPTH)
            .map(|_| CallDepthGuard::enter().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            method
                .call(
                    Value::Object(constructor.clone()),
                    std::slice::from_ref(&value)
                )
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::RangeError
        );
        assert_eq!(
            constructor
                .try_default_instance_of(&realm, &value, &key)
                .unwrap()
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::RangeError
        );
        drop(guards);
        assert!(
            constructor
                .try_default_instance_of(&realm, &value, &key)
                .unwrap()
                .unwrap()
        );
        constructor
            .create_data_property(key.clone(), Value::Undefined)
            .unwrap();
        assert!(
            constructor
                .try_default_instance_of(&realm, &value, &key)
                .is_none()
        );
    }

    #[test]
    fn default_instance_direct_and_shortcut_errors_keep_the_defining_realm() {
        let context = AllocationContext::new();
        let first = Realm::ordinary_in(&context);
        let second = Realm::ordinary_in(&context);
        let prototype = second.intrinsic(IntrinsicId::FunctionPrototype);
        let constructor =
            Object::native_function_with_metadata(&prototype, "C", 0, |_, _| Ok(Value::Undefined));
        constructor
            .create_data_property("prototype".into(), Value::Number(0.0))
            .unwrap();
        let key = second
            .well_known_symbol(WellKnownSymbol::HasInstance)
            .into();
        let value = Value::Object(Object::new_in(&context));
        let Value::Object(method) = prototype.get(&key).unwrap() else {
            panic!("builtin")
        };
        let direct = method
            .call(
                Value::Object(constructor.clone()),
                std::slice::from_ref(&value),
            )
            .unwrap_err();
        let shortcut = constructor
            .try_default_instance_of(&second, &value, &key)
            .unwrap()
            .unwrap_err();
        assert!(
            constructor
                .try_default_instance_of(&first, &value, &key)
                .is_none()
        );
        // Realm symbol registries are currently distinct. Explicitly expose
        // the foreign builtin under the caller's key to test its error Realm
        // independently of that existing symbol-identity limitation.
        constructor
            .create_data_property(
                first.well_known_symbol(WellKnownSymbol::HasInstance).into(),
                Value::Object(method.clone()),
            )
            .unwrap();
        let crossing = crate::runtime::operators::object_binary(
            &first,
            "instanceof",
            value,
            Value::Object(constructor),
        )
        .unwrap_err();
        for (index, error) in [direct, shortcut, crossing].into_iter().enumerate() {
            assert_eq!(error.kind(), RuntimeErrorKind::TypeError);
            let Some(Value::Object(exception)) = error.thrown_value() else {
                panic!("defining Realm TypeError case {index}")
            };
            assert_eq!(
                exception.try_prototype().unwrap(),
                Some(second.intrinsic(IntrinsicId::TypeErrorPrototype))
            );
        }
    }
}
