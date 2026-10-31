//! ECMAScript language values and fundamental abstract conversion operations.
//!
//! Strings intentionally store UTF-16 code units, so lone surrogates are not
//! lost. Object allocation currently uses reference counting; tracing and the
//! final heap representation belong to the garbage-collector milestone.

use std::cell::RefCell;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

use num_bigint::BigInt;

pub(crate) mod array_literal;
mod array_methods;
mod binding;
mod builtins;
pub(crate) mod bytecode_function;
mod class;
mod class_fields;
mod coercion;
mod collections;
mod elements;
mod environment;
mod execution;
mod function;
mod generator;
mod host_data;
mod indexed;
mod interpreter;
mod intrinsics;
pub(crate) mod iterator;
mod job_queue;
mod managed;
mod native_function;
mod numeric;
mod object;
pub(crate) mod object_literal;
mod operators;
mod pipeline;
mod private_fields;
mod promise;
mod proxy;
mod regexp;
mod statement;
mod stored;
mod string;
mod string_methods;

pub use collections::{
    MapIterator, MapObject, SetIterator, SetObject, WeakMapObject, WeakSetObject,
};
pub use environment::{Environment, EnvironmentKind, ObjectEnvironmentOptions, ThisBindingStatus};
pub use execution::{Agent, ContextState, ExecutionContext, Realm, ScriptRecord};
pub use function::FunctionKind;
pub use generator::{Generator, GeneratorState, IteratorResult};
pub use host_data::{DateObject, HostHooks, IntlProvider, Json, JsonTransform, UnsupportedIntl};
pub use indexed::{ArrayBuffer, ArrayObject, DataView, TypedArray};
pub(crate) use interpreter::unary_numeric_value;
pub use interpreter::{ExpressionEvaluator, Reference, ReferencedName};
pub(crate) use interpreter::{
    PropertyReferenceContext, binary_number, binary_values, binary_values_in_realm,
    parse_numeric_literal, type_of, update_reference_in_realm,
};
pub use intrinsics::{IntrinsicId, IntrinsicRegistry, WellKnownSymbol, WellKnownSymbols};
pub use job_queue::JobQueue;
pub(crate) use managed::Runtime;
/// Explicit owner for a traced runtime heap and its host-created objects.
pub use managed::Runtime as AllocationContext;
pub use numeric::{BigIntObject, Math, NumberObject, parse_float, parse_int};
pub use object::{PropertyDescriptor, PropertyKey, is_compatible_property_descriptor};
pub use pipeline::{ModuleGraph, ModuleStatus, SourceTextModule};
pub use promise::{Promise, PromiseState};
pub use proxy::ProxyObject;
pub use regexp::{RegExp, RegExpFlags, RegExpMatch, StringObject};
pub use statement::{Completion, StatementEvaluator};

pub use string::JsString;

#[derive(Debug)]
struct SymbolData {
    id: u64,
    description: Option<JsString>,
}

/// A unique JavaScript Symbol value.
#[derive(Clone, Debug)]
pub struct Symbol(Rc<SymbolData>);

impl Symbol {
    /// Creates a fresh symbol with an optional description.
    #[must_use]
    pub fn new(description: Option<JsString>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = next_identity(&NEXT_ID, "JavaScript symbol");
        Self(Rc::new(SymbolData { id, description }))
    }

    /// Returns the descriptive string, if one was supplied.
    #[must_use]
    pub fn description(&self) -> Option<&JsString> {
        self.0.description.as_ref()
    }

    /// Returns an engine-local identity useful for diagnostics and keyed storage.
    #[must_use]
    pub fn identity(&self) -> u64 {
        self.0.id
    }
}

impl PartialEq for Symbol {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Symbol {}

/// The hint supplied to ECMAScript's `ToPrimitive` operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PrimitiveHint {
    /// Let an exotic conversion method choose; ordinary objects use number order.
    #[default]
    Default,
    /// Prefer numeric conversion (`valueOf` before `toString`).
    Number,
    /// Prefer string conversion (`toString` before `valueOf`).
    String,
}

#[derive(Clone)]
enum ErrorMessage {
    Owned(String),
    Shared(Rc<String>),
}
impl std::ops::Deref for ErrorMessage {
    type Target = str;
    fn deref(&self) -> &str {
        match self {
            Self::Owned(text) => text.as_str(),
            Self::Shared(text) => text.as_str(),
        }
    }
}
impl fmt::Debug for ErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}
impl fmt::Display for ErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self)
    }
}
impl PartialEq for ErrorMessage {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}
impl Eq for ErrorMessage {}

#[derive(Clone)]
struct NativeRuntimeError {
    kind: RuntimeErrorKind,
    message: ErrorMessage,
    thrown: Option<Box<Value>>,
}
#[derive(Clone)]
enum RuntimeErrorData {
    Native(NativeRuntimeError),
    Thrown(Value),
}

/// Runtime failures raised by abstract operations.
#[derive(Clone)]
pub struct RuntimeError(RuntimeErrorData);

impl RuntimeError {
    fn native(kind: RuntimeErrorKind, message: ErrorMessage, thrown: Option<Box<Value>>) -> Self {
        Self(RuntimeErrorData::Native(NativeRuntimeError {
            kind,
            message,
            thrown,
        }))
    }
    fn reference_error_shared(message: Rc<String>) -> Self {
        Self::native(
            RuntimeErrorKind::ReferenceError,
            ErrorMessage::Shared(message),
            None,
        )
    }
    fn attach_thrown(&mut self, value: Value) {
        match &mut self.0 {
            RuntimeErrorData::Native(error) => error.thrown = Some(Box::new(value)),
            RuntimeErrorData::Thrown(current) => *current = value,
        }
    }
    /// Creates a TypeError completion.
    #[must_use]
    pub fn type_error(message: impl Into<String>) -> Self {
        Self::native(
            RuntimeErrorKind::TypeError,
            ErrorMessage::Owned(message.into()),
            None,
        )
    }
    /// Creates a SyntaxError completion.
    #[must_use]
    pub fn syntax_error(message: impl Into<String>) -> Self {
        Self::native(
            RuntimeErrorKind::SyntaxError,
            ErrorMessage::Owned(message.into()),
            None,
        )
    }
    /// Creates a ReferenceError completion.
    #[must_use]
    pub fn reference_error(message: impl Into<String>) -> Self {
        Self::native(
            RuntimeErrorKind::ReferenceError,
            ErrorMessage::Owned(message.into()),
            None,
        )
    }
    /// Creates a RangeError completion.
    #[must_use]
    pub fn range_error(message: impl Into<String>) -> Self {
        Self::native(
            RuntimeErrorKind::RangeError,
            ErrorMessage::Owned(message.into()),
            None,
        )
    }
    /// Creates an engine-internal invariant error.
    #[must_use]
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::native(
            RuntimeErrorKind::InternalError,
            ErrorMessage::Owned(message.into()),
            None,
        )
    }
    /// Carries an arbitrary JavaScript throw value without coercion or boxing.
    #[must_use]
    pub fn thrown(value: Value) -> Self {
        Self(RuntimeErrorData::Thrown(value))
    }
    /// Returns the original throw or the native exception in its originating Realm.
    #[must_use]
    pub fn thrown_value(&self) -> Option<&Value> {
        match &self.0 {
            RuntimeErrorData::Native(error) => error.thrown.as_deref(),
            RuntimeErrorData::Thrown(value) => Some(value),
        }
    }
    /// Returns the error category.
    #[must_use]
    pub fn kind(&self) -> RuntimeErrorKind {
        match &self.0 {
            RuntimeErrorData::Native(error) => error.kind,
            RuntimeErrorData::Thrown(_) => RuntimeErrorKind::ThrownValue,
        }
    }
    /// Returns the human-readable diagnostic.
    #[must_use]
    pub fn message(&self) -> &str {
        match &self.0 {
            RuntimeErrorData::Native(error) => &error.message,
            RuntimeErrorData::Thrown(_) => "JavaScript throw completion",
        }
    }
}
impl fmt::Debug for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeError")
            .field("kind", &self.kind())
            .field("message", &self.message())
            .field("thrown", &self.thrown_value())
            .finish()
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind(), self.message())
    }
}

impl std::error::Error for RuntimeError {}

impl PartialEq for RuntimeError {
    fn eq(&self, other: &Self) -> bool {
        self.kind() == other.kind()
            && self.message() == other.message()
            && match (self.thrown_value(), other.thrown_value()) {
                (None, None) => true,
                (Some(left), Some(right)) => left.same_value(right),
                _ => false,
            }
    }
}

impl Eq for RuntimeError {}

/// Categories of runtime errors currently produced by abstract operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeErrorKind {
    /// An explicit JavaScript throw, whose value is retained separately.
    ThrownValue,
    /// An operation was applied to an incompatible ECMAScript value.
    TypeError,
    /// Text could not be parsed using the required ECMAScript grammar.
    SyntaxError,
    /// A binding is absent or cannot yet be accessed.
    ReferenceError,
    /// A numeric value falls outside the operation's permitted range.
    RangeError,
    /// An invalid engine state transition was requested.
    InternalError,
}

impl fmt::Display for RuntimeErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ThrownValue => formatter.write_str("JavaScript throw"),
            Self::TypeError => formatter.write_str("TypeError"),
            Self::SyntaxError => formatter.write_str("SyntaxError"),
            Self::ReferenceError => formatter.write_str("ReferenceError"),
            Self::RangeError => formatter.write_str("RangeError"),
            Self::InternalError => formatter.write_str("InternalError"),
        }
    }
}

/// A result produced by a runtime abstract operation.
pub type RuntimeResult<T> = Result<T, RuntimeError>;

type ConversionHook = Rc<dyn Fn(PrimitiveHint) -> RuntimeResult<Value>>;
type OrdinaryConversionHook = Rc<dyn Fn() -> RuntimeResult<Value>>;

#[derive(Default)]
struct ObjectConversion {
    exotic: Option<ConversionHook>,
    value_of: Option<OrdinaryConversionHook>,
    to_string: Option<OrdinaryConversionHook>,
}

struct ObjectData {
    id: u64,
    // Non-owning facade metadata; never makes the allocation a GC root.
    facade: RefCell<Weak<managed::ObjectRoot>>,
    conversion: RefCell<ObjectConversion>,
    state: RefCell<object::ObjectState>,
}

impl ObjectData {
    fn ordinary() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: next_identity(&NEXT_ID, "JavaScript object"),
            facade: RefCell::new(Weak::new()),
            conversion: RefCell::new(ObjectConversion::default()),
            state: RefCell::new(object::ordinary_state()),
        }
    }
}

/// An identity-bearing ordinary object with configurable primitive-conversion hooks.
///
/// Properties and prototype internal methods are added by the object-model layer.
#[derive(Clone)]
pub struct Object(managed::ObjectReference);

#[derive(Clone)]
pub(super) struct WeakObject(managed::WeakObjectRoot);

impl Object {
    pub(super) fn downgrade(&self) -> WeakObject {
        WeakObject(managed::WeakObjectRoot::new(self))
    }
    /// Allocates an ordinary object in an explicit shared context.
    pub fn new_in(runtime: &AllocationContext) -> Self {
        runtime.allocate_object(ObjectData::ordinary())
    }

    /// Borrows the shared context that owns this object.
    pub fn allocation_context(&self) -> &AllocationContext {
        self.0
            .runtime()
            .expect("objects have an allocation context")
    }

    pub(crate) fn new_for(context: &Object) -> Self {
        Self::new_in(context.allocation_context())
    }

    pub(crate) fn check_allocation_context(&self, value: &Object) -> RuntimeResult<()> {
        stored::StoredObject::check_context(self, value)
    }

    fn check_abrupt_context(&self, error: &RuntimeError) -> RuntimeResult<()> {
        if self.0.runtime().is_some() {
            if let Some(Value::Object(value)) = error.thrown_value() {
                self.check_allocation_context(value)?;
            }
        }
        Ok(())
    }

    /// Returns an engine-local identity useful for diagnostics and keyed storage.
    #[must_use]
    pub fn identity(&self) -> u64 {
        self.0.id
    }

    /// Installs the object's `@@toPrimitive` equivalent.
    pub fn set_to_primitive_hook(
        &self,
        hook: impl Fn(PrimitiveHint) -> RuntimeResult<Value> + 'static,
    ) {
        self.0.conversion.borrow_mut().exotic = Some(Rc::new(hook));
    }

    /// Installs an ordinary `valueOf` conversion hook.
    pub fn set_value_of_hook(&self, hook: impl Fn() -> RuntimeResult<Value> + 'static) {
        self.0.conversion.borrow_mut().value_of = Some(Rc::new(hook));
    }

    /// Installs an ordinary `toString` conversion hook.
    pub fn set_to_string_hook(&self, hook: impl Fn() -> RuntimeResult<Value> + 'static) {
        self.0.conversion.borrow_mut().to_string = Some(Rc::new(hook));
    }
}

impl WeakObject {
    #[cfg(test)]
    pub(super) fn upgrade(&self) -> Option<Object> {
        self.0.upgrade()
    }
}

impl fmt::Debug for Object {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Object")
            .field("identity", &self.identity())
            .finish_non_exhaustive()
    }
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        self.0.same_object(&other.0)
    }
}

impl Eq for Object {}

/// An ECMAScript language value.
#[derive(Clone, Debug)]
pub enum Value {
    /// The `undefined` value.
    Undefined,
    /// The `null` value.
    Null,
    /// A Boolean value.
    Boolean(bool),
    /// A UTF-16 String value.
    String(JsString),
    /// A unique Symbol value.
    Symbol(Symbol),
    /// An IEEE-754 binary64 Number value.
    Number(f64),
    /// An arbitrary-precision integer value.
    BigInt(BigInt),
    /// An identity-bearing Object value.
    Object(Object),
}

/// The result of `ToNumeric`, which preserves the Number/BigInt distinction.
#[derive(Clone, Debug, PartialEq)]
pub enum Numeric {
    /// An IEEE-754 binary64 value.
    Number(f64),
    /// An arbitrary-precision integer.
    BigInt(BigInt),
}

impl Value {
    /// Implements ECMAScript `SameValue` (`Object.is`).
    #[must_use]
    pub fn same_value(&self, other: &Self) -> bool {
        self.same_value_impl(other, false)
    }

    /// Implements ECMAScript `SameValueZero` (used by Map and Set).
    #[must_use]
    pub fn same_value_zero(&self, other: &Self) -> bool {
        self.same_value_impl(other, true)
    }

    fn same_value_impl(&self, other: &Self, zero_equal: bool) -> bool {
        match (self, other) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Boolean(left), Self::Boolean(right)) => left == right,
            (Self::String(left), Self::String(right)) => left == right,
            (Self::Symbol(left), Self::Symbol(right)) => left == right,
            (Self::BigInt(left), Self::BigInt(right)) => left == right,
            (Self::Object(left), Self::Object(right)) => left == right,
            (Self::Number(left), Self::Number(right)) => {
                (left.is_nan() && right.is_nan())
                    || (left == right
                        && (zero_equal
                            || *left != 0.0
                            || left.is_sign_positive() == right.is_sign_positive()))
            }
            _ => false,
        }
    }

    /// Implements ECMAScript `ToBoolean`.
    #[must_use]
    pub fn to_boolean(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Boolean(value) => *value,
            Self::String(value) => !value.is_empty(),
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::BigInt(value) => *value != BigInt::from(0),
            Self::Symbol(_) | Self::Object(_) => true,
        }
    }

    /// Implements ECMAScript `ToPrimitive`, including observable hook ordering.
    pub fn to_primitive(&self, hint: PrimitiveHint) -> RuntimeResult<Self> {
        let Self::Object(object) = self else {
            return Ok(self.clone());
        };

        let exotic = object.0.conversion.borrow().exotic.clone();
        if let Some(hook) = exotic {
            let result = hook(hint);
            if let Err(error) = &result {
                object.check_abrupt_context(error)?;
            }
            let result = result?;
            if matches!(result, Self::Object(_)) {
                return Err(RuntimeError::type_error("@@toPrimitive returned an object"));
            }
            return Ok(result);
        }

        let ordinary_hint = match hint {
            PrimitiveHint::String => PrimitiveHint::String,
            PrimitiveHint::Default | PrimitiveHint::Number => PrimitiveHint::Number,
        };
        let conversion = object.0.conversion.borrow();
        let value_of = conversion.value_of.clone();
        let to_string = conversion.to_string.clone();
        drop(conversion);
        let methods = match ordinary_hint {
            PrimitiveHint::String => [(to_string, true), (value_of, false)],
            PrimitiveHint::Default | PrimitiveHint::Number => {
                [(value_of, false), (to_string, true)]
            }
        };
        for (hook, string_fallback) in methods {
            let result = if let Some(hook) = hook {
                let result = hook();
                if let Err(error) = &result {
                    object.check_abrupt_context(error)?;
                }
                result?
            } else if string_fallback {
                Self::String(JsString::from("[object Object]"))
            } else {
                Self::Object(object.clone())
            };
            if !matches!(result, Self::Object(_)) {
                return Ok(result);
            }
        }
        Err(RuntimeError::type_error(
            "cannot convert object to a primitive value",
        ))
    }

    /// Implements ECMAScript `ToNumber`.
    pub fn to_number(&self) -> RuntimeResult<f64> {
        match self.to_primitive(PrimitiveHint::Number)? {
            Self::Undefined => Ok(f64::NAN),
            Self::Null => Ok(0.0),
            Self::Boolean(value) => Ok(if value { 1.0 } else { 0.0 }),
            Self::String(value) => Ok(string_to_number(&value)),
            Self::Number(value) => Ok(value),
            Self::BigInt(_) => Err(RuntimeError::type_error("cannot convert BigInt to Number")),
            Self::Symbol(_) => Err(RuntimeError::type_error("cannot convert Symbol to Number")),
            Self::Object(_) => unreachable!("ToPrimitive returned an object"),
        }
    }

    /// Implements ECMAScript `ToNumeric`.
    pub fn to_numeric(&self) -> RuntimeResult<Numeric> {
        let primitive = self.to_primitive(PrimitiveHint::Number)?;
        if let Self::BigInt(value) = primitive {
            Ok(Numeric::BigInt(value))
        } else {
            primitive.to_number().map(Numeric::Number)
        }
    }

    /// Implements ECMAScript `ToString`, preserving lone UTF-16 surrogates.
    pub fn to_js_string(&self) -> RuntimeResult<JsString> {
        match self.to_primitive(PrimitiveHint::String)? {
            Self::Undefined => Ok(JsString::from("undefined")),
            Self::Null => Ok(JsString::from("null")),
            Self::Boolean(false) => Ok(JsString::from("false")),
            Self::Boolean(true) => Ok(JsString::from("true")),
            Self::String(value) => Ok(value),
            Self::Number(value) => Ok(number_to_js_string(value)),
            Self::BigInt(value) => Ok(JsString::from(value.to_string())),
            Self::Symbol(_) => Err(RuntimeError::type_error("cannot convert Symbol to String")),
            Self::Object(_) => unreachable!("ToPrimitive returned an object"),
        }
    }

    /// Implements ECMAScript `ToBigInt`.
    pub fn to_bigint(&self) -> RuntimeResult<BigInt> {
        match self.to_primitive(PrimitiveHint::Number)? {
            Self::Boolean(value) => Ok(BigInt::from(value)),
            Self::BigInt(value) => Ok(value),
            Self::String(value) => string_to_bigint(&value)
                .ok_or_else(|| RuntimeError::syntax_error("string is not a valid BigInt")),
            Self::Undefined | Self::Null | Self::Number(_) | Self::Symbol(_) => Err(
                RuntimeError::type_error("value cannot be converted to BigInt"),
            ),
            Self::Object(_) => unreachable!("ToPrimitive returned an object"),
        }
    }
}

fn utf16_to_scalar_string(value: &JsString) -> Option<String> {
    char::decode_utf16(value.code_units().iter().copied())
        .collect::<Result<String, _>>()
        .ok()
}

fn next_identity(counter: &AtomicU64, kind: &str) -> u64 {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .unwrap_or_else(|| panic!("exhausted {kind} identities"));
        match counter.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return current,
            Err(observed) => current = observed,
        }
    }
}

fn trim_string_numeric(value: &str) -> &str {
    value.trim_matches(is_ecmascript_whitespace)
}

fn is_ecmascript_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{0009}' | '\u{000b}' | '\u{000c}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
                | '\n'
                | '\r'
    )
}

fn string_to_number(value: &JsString) -> f64 {
    let Some(value) = utf16_to_scalar_string(value) else {
        return f64::NAN;
    };
    let value = trim_string_numeric(&value);
    if value.is_empty() {
        return 0.0;
    }
    match value {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if let Some((radix, digits)) = prefixed_integer(value) {
        return BigInt::parse_bytes(digits.as_bytes(), radix)
            .and_then(|integer| integer.to_string().parse::<f64>().ok())
            .unwrap_or(f64::NAN);
    }
    if is_decimal_string_numeric_literal(value) {
        value.parse::<f64>().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

fn is_decimal_string_numeric_literal(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut cursor = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let integer_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let integer_digits = cursor - integer_start;
    let mut fraction_digits = 0;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let fraction_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        fraction_digits = cursor - fraction_start;
    }
    if integer_digits + fraction_digits == 0 {
        return false;
    }
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let exponent_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == exponent_start {
            return false;
        }
    }
    cursor == bytes.len()
}

fn string_to_bigint(value: &JsString) -> Option<BigInt> {
    let value = utf16_to_scalar_string(value)?;
    let value = trim_string_numeric(&value);
    if value.is_empty() {
        return Some(BigInt::from(0));
    }
    if let Some((radix, digits)) = prefixed_integer(value) {
        return BigInt::parse_bytes(digits.as_bytes(), radix);
    }
    if value.starts_with('+') || value.starts_with('-') {
        let (sign, digits) = value.split_at(1);
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let magnitude = BigInt::parse_bytes(digits.as_bytes(), 10)?;
        return Some(if sign == "-" { -magnitude } else { magnitude });
    }
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    BigInt::parse_bytes(value.as_bytes(), 10)
}

fn prefixed_integer(value: &str) -> Option<(u32, &str)> {
    let (radix, digits) = if let Some(digits) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        (16, digits)
    } else if let Some(digits) = value
        .strip_prefix("0o")
        .or_else(|| value.strip_prefix("0O"))
    {
        (8, digits)
    } else {
        let digits = value
            .strip_prefix("0b")
            .or_else(|| value.strip_prefix("0B"))?;
        (2, digits)
    };
    (!digits.is_empty()).then_some((radix, digits))
}

fn number_to_js_string(value: f64) -> JsString {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == 0.0 {
        return "0".into();
    }
    if value == f64::INFINITY {
        return "Infinity".into();
    }
    if value == f64::NEG_INFINITY {
        return "-Infinity".into();
    }
    JsString::from(ryu_js::Buffer::new().format(value))
}

#[cfg(test)]
mod shared_runtime_message_tests {
    use super::*;
    #[test]
    fn shared_messages_preserve_error_kinds_values_and_bound_storage() {
        for i in 0..80 {
            let text = format!("diagnostic {i} 日本語😀");
            let a = RuntimeError::type_error(text.clone());
            let b = RuntimeError::syntax_error(text.clone());
            assert_eq!(a.message(), text);
            assert_eq!(b.message(), text);
            assert_ne!(a, b);
            assert_eq!(a, a.clone());
            let owned = RuntimeError::reference_error(text.clone());
            let shared = RuntimeError::reference_error_shared(Rc::new(text));
            assert_eq!(owned, shared);
            assert_eq!(format!("{owned:?}"), format!("{shared:?}"));
        }
        let mut oversized = String::with_capacity(4096);
        oversized.push_str("small");
        assert_eq!(RuntimeError::reference_error(oversized).message(), "small");
        for value in [Value::Undefined, Value::Null, Value::Number(-0.0)] {
            let error = RuntimeError::thrown(value.clone());
            assert!(error.thrown_value().unwrap().same_value(&value));
            assert_eq!(error, error.clone());
        }
    }
}
