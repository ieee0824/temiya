//! Date, JSON, and explicit host/Intl boundaries.

use std::{
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use super::stored::StoredObject;
use super::{ArrayObject, JsString, Object, PropertyKey, RuntimeError, RuntimeResult, Value};

/// Host-dependent services used by Realm built-ins.
#[derive(Clone)]
pub struct HostHooks {
    now: Rc<dyn Fn() -> f64>,
}

impl HostHooks {
    /// Creates hooks with an injected UTC millisecond clock.
    #[must_use]
    pub fn new(now: impl Fn() -> f64 + 'static) -> Self {
        Self { now: Rc::new(now) }
    }
    /// Reads UTC milliseconds from the host clock.
    #[must_use]
    pub fn now(&self) -> f64 {
        (self.now)()
    }
}

impl Default for HostHooks {
    fn default() -> Self {
        Self::new(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(f64::NAN, |duration| duration.as_secs_f64() * 1000.0)
        })
    }
}

/// A Date object storing a clipped UTC millisecond time value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DateObject(Object);

pub(super) struct DateFactory {
    prototype: StoredObject,
    // Explicit external host state: managed captures belong in traced fields.
    host: HostHooks,
}
impl DateFactory {
    pub(super) fn install(
        constructor: &Object,
        prototype: &Object,
        host: HostHooks,
    ) -> RuntimeResult<()> {
        let factory = Self {
            prototype: StoredObject::store(constructor, prototype)?,
            host,
        };
        constructor.set_managed_callable(super::object::ManagedCallable::Date(factory));
        Ok(())
    }
    pub(super) fn call(&self) -> RuntimeResult<Value> {
        DateObject::format_value(DateObject::clip(self.host.now())).map(Value::String)
    }
    pub(super) fn construct(&self, owner: &Object, arguments: &[Value]) -> RuntimeResult<Object> {
        let prototype = self.prototype.load(owner)?;
        let value = match arguments.first() {
            Some(value) => value.to_number()?,
            None => self.host.now(),
        };
        let object = DateObject::new_for(&prototype, value).object();
        object.initialize_prototype(Some(prototype));
        Ok(object)
    }
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        self.prototype.trace(tracer);
    }
}

impl DateObject {
    /// Creates a Date after applying the ECMAScript TimeClip boundary.
    #[must_use]
    pub fn new(context: &super::AllocationContext, milliseconds: f64) -> Self {
        Self::initialize(Object::new_in(context), milliseconds)
    }
    pub(crate) fn new_for(context: &Object, milliseconds: f64) -> Self {
        Self::initialize(Object::new_for(context), milliseconds)
    }
    fn clip(milliseconds: f64) -> f64 {
        if milliseconds.is_finite() && milliseconds.abs() <= 8.64e15 {
            milliseconds.trunc()
        } else {
            f64::NAN
        }
    }
    fn initialize(object: Object, milliseconds: f64) -> Self {
        object.0.state.borrow_mut().date_value = Some(Self::clip(milliseconds));
        Self(object)
    }
    /// Creates a Date using an explicit host clock.
    #[must_use]
    pub fn now(context: &super::AllocationContext, host: &HostHooks) -> Self {
        Self::new(context, host.now())
    }
    /// Wraps an object with Date internal slots.
    #[must_use]
    pub fn from_object(object: Object) -> Option<Self> {
        let present = object.0.state.borrow().date_value.is_some();
        present.then_some(Self(object))
    }
    /// Returns the underlying object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }
    /// Returns the clipped UTC millisecond value.
    #[must_use]
    pub fn value(&self) -> f64 {
        self.0.0.state.borrow().date_value.expect("Date value")
    }
    /// Formats an ISO-8601 UTC string or returns RangeError for an invalid Date.
    pub fn to_iso_string(&self) -> RuntimeResult<JsString> {
        Self::format_value(self.value())
    }
    fn format_value(value: f64) -> RuntimeResult<JsString> {
        if value.is_nan() {
            return Err(RuntimeError::range_error("invalid Date"));
        }
        let milliseconds = value as i64;
        let days = milliseconds.div_euclid(86_400_000);
        let within = milliseconds.rem_euclid(86_400_000);
        let (year, month, day) = civil_from_days(days);
        let hour = within / 3_600_000;
        let minute = within / 60_000 % 60;
        let second = within / 1_000 % 60;
        let milli = within % 1_000;
        let year = if (0..=9999).contains(&year) {
            format!("{year:04}")
        } else {
            format!("{year:+07}")
        };
        Ok(JsString::from(format!(
            "{year}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z"
        )))
    }
}

/// JSON parsing and serialization over runtime Values.
pub struct Json;

pub(super) struct JsonParseBuiltin {
    object_prototype: StoredObject,
    array_prototype: StoredObject,
}
impl JsonParseBuiltin {
    pub(super) fn create(intrinsics: &super::IntrinsicRegistry) -> RuntimeResult<Object> {
        let function = intrinsics.get(super::IntrinsicId::FunctionPrototype);
        let builtin = Self {
            object_prototype: StoredObject::initial(
                function.0.runtime(),
                &intrinsics.get(super::IntrinsicId::ObjectPrototype),
            )?,
            array_prototype: StoredObject::initial(
                function.0.runtime(),
                &intrinsics.get(super::IntrinsicId::ArrayPrototype),
            )?,
        };
        Ok(Object::managed_function_with_metadata(
            &function,
            "parse",
            2,
            super::object::ManagedCallable::JsonParse(builtin),
        ))
    }
    pub(super) fn call(&self, owner: &Object, arguments: &[Value]) -> RuntimeResult<Value> {
        let source = arguments
            .first()
            .unwrap_or(&Value::Undefined)
            .to_js_string()?;
        let parsed = parse_json(&source)?;
        let prototypes = if matches!(
            parsed,
            serde_json::Value::Array(_) | serde_json::Value::Object(_)
        ) {
            Some((
                self.object_prototype.load(owner)?,
                self.array_prototype.load(owner)?,
            ))
        } else {
            None
        };
        let value = from_json(parsed, &|length| {
            let (object_prototype, array_prototype) = prototypes
                .as_ref()
                .expect("JSON containers initialize their prototypes");
            if let Some(length) = length {
                let object = ArrayObject::new_for(array_prototype, length)?.object();
                object.initialize_prototype(Some(array_prototype.clone()));
                Ok(object)
            } else {
                let object = Object::new_for(object_prototype);
                object.initialize_prototype(Some(object_prototype.clone()));
                Ok(object)
            }
        })?;
        let Some(Value::Object(reviver)) = arguments
            .get(1)
            .filter(|value| matches!(value, Value::Object(object) if object.is_callable()))
        else {
            return Ok(value);
        };
        revive("", value, &mut |key, value| {
            reviver.call(Value::Undefined, &[Value::String(key.into()), value])
        })
    }
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        self.object_prototype.trace(tracer);
        self.array_prototype.trace(tracer);
    }
}

/// Callback shape used by JSON reviver and replacer operations.
pub type JsonTransform<'a> = dyn FnMut(&str, Value) -> RuntimeResult<Value> + 'a;

impl Json {
    /// Parses JSON and optionally walks it bottom-up with a reviver.
    pub fn parse(
        context: &super::AllocationContext,
        source: &JsString,
        mut reviver: Option<&mut JsonTransform<'_>>,
    ) -> RuntimeResult<Value> {
        let value = from_json(parse_json(source)?, &|length| {
            if let Some(length) = length {
                ArrayObject::new(context, length).map(|array| array.object())
            } else {
                Ok(Object::new_in(context))
            }
        })?;
        let value = if let Some(reviver) = reviver.as_mut() {
            revive("", value, *reviver)?
        } else {
            value
        };
        if let Value::Object(object) = &value {
            StoredObject::initial(Some(context), object)?;
        }
        Ok(value)
    }

    /// Serializes a runtime Value, optionally transforming properties with a replacer.
    pub fn stringify(
        value: Value,
        mut replacer: Option<&mut JsonTransform<'_>>,
    ) -> RuntimeResult<Option<JsString>> {
        let mut stack = Vec::new();
        serialize(&value, "", &mut replacer, &mut stack, false)
            .map(|value| value.map(JsString::from))
    }
}

/// Locale-sensitive services supplied outside the core runtime.
pub trait IntlProvider {
    /// Formats a Number for a locale identifier.
    fn format_number(&self, locale: &str, value: f64) -> RuntimeResult<JsString>;
}

/// Default provider documenting that Intl data is not bundled in the engine.
pub struct UnsupportedIntl;
impl IntlProvider for UnsupportedIntl {
    fn format_number(&self, _locale: &str, _value: f64) -> RuntimeResult<JsString> {
        Err(RuntimeError::type_error("Intl provider is not configured"))
    }
}

fn parse_json(source: &JsString) -> RuntimeResult<serde_json::Value> {
    serde_json::from_str(&source.to_string_lossy())
        .map_err(|error| RuntimeError::syntax_error(error.to_string()))
}

fn from_json(
    value: serde_json::Value,
    allocate: &impl Fn(Option<u32>) -> RuntimeResult<Object>,
) -> RuntimeResult<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(value) => Value::Boolean(value),
        serde_json::Value::Number(value) => Value::Number(
            value
                .as_f64()
                .ok_or_else(|| RuntimeError::range_error("JSON number is out of range"))?,
        ),
        serde_json::Value::String(value) => Value::String(JsString::from(value)),
        serde_json::Value::Array(values) => {
            let length = u32::try_from(values.len())
                .map_err(|_| RuntimeError::range_error("JSON array is too large"))?;
            let array = allocate(Some(length))?;
            for (index, value) in values.into_iter().enumerate() {
                array.create_data_property(
                    JsString::from(index.to_string()).into(),
                    from_json(value, allocate)?,
                )?;
            }
            Value::Object(array)
        }
        serde_json::Value::Object(values) => {
            let object = allocate(None)?;
            for (key, value) in values {
                object.create_data_property(
                    JsString::from(key).into(),
                    from_json(value, allocate)?,
                )?;
            }
            Value::Object(object)
        }
    })
}

fn revive(
    key: &str,
    value: Value,
    reviver: &mut dyn FnMut(&str, Value) -> RuntimeResult<Value>,
) -> RuntimeResult<Value> {
    let value = if let Value::Object(object) = value {
        let keys = if let Some(array) = ArrayObject::from_object(object.clone()) {
            (0..array.length())
                .map(|index| JsString::from(index.to_string()))
                .collect()
        } else {
            object.enumerable_own_string_keys()?
        };
        for name in keys {
            let text = name.to_string_lossy();
            let child = object.get(&PropertyKey::String(name.clone()))?;
            let revived = revive(&text, child, reviver)?;
            if matches!(revived, Value::Undefined) {
                object.delete(&PropertyKey::String(name));
            } else {
                object.set(&PropertyKey::String(name), revived)?;
            }
        }
        Value::Object(object)
    } else {
        value
    };
    reviver(key, value)
}

fn serialize(
    value: &Value,
    key: &str,
    replacer: &mut Option<&mut JsonTransform<'_>>,
    stack: &mut Vec<u64>,
    array_element: bool,
) -> RuntimeResult<Option<String>> {
    let replaced = if let Some(callback) = replacer.as_mut() {
        (*callback)(key, value.clone())?
    } else {
        value.clone()
    };
    Ok(match replaced {
        Value::Undefined | Value::Symbol(_) => array_element.then(|| "null".to_owned()),
        Value::BigInt(_) => {
            return Err(RuntimeError::type_error(
                "BigInt cannot be serialized as JSON",
            ));
        }
        Value::Null => Some("null".to_owned()),
        Value::Boolean(value) => Some(value.to_string()),
        Value::Number(value) if !value.is_finite() => Some("null".to_owned()),
        Value::Number(value) => Some(Value::Number(value).to_js_string()?.to_string_lossy()),
        Value::String(value) => {
            Some(serde_json::to_string(&value.to_string_lossy()).expect("string serialization"))
        }
        Value::Object(object) => {
            if stack.contains(&object.identity()) {
                return Err(RuntimeError::type_error("cyclic JSON value"));
            }
            stack.push(object.identity());
            let result = if let Some(array) = ArrayObject::from_object(object.clone()) {
                let mut items = Vec::new();
                for index in 0..array.length() {
                    let key = index.to_string();
                    items.push(
                        serialize(
                            &object.get(&JsString::from(key.clone()).into())?,
                            &key,
                            replacer,
                            stack,
                            true,
                        )?
                        .unwrap_or_else(|| "null".to_owned()),
                    );
                }
                format!("[{}]", items.join(","))
            } else {
                let mut items = Vec::new();
                for property in object.own_property_keys() {
                    let PropertyKey::String(name) = property else {
                        continue;
                    };
                    if object
                        .get_own_property(&PropertyKey::String(name.clone()))
                        .is_some_and(|descriptor| descriptor.enumerable != Some(true))
                    {
                        continue;
                    }
                    let key = name.to_string_lossy();
                    if let Some(value) = serialize(
                        &object.get(&PropertyKey::String(name))?,
                        &key,
                        replacer,
                        stack,
                        false,
                    )? {
                        items.push(format!(
                            "{}:{value}",
                            serde_json::to_string(&key).expect("key serialization")
                        ));
                    }
                }
                format!("{{{}}}", items.join(","))
            };
            stack.pop();
            Some(result)
        }
    })
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod managed_tests {
    use super::*;

    #[test]
    fn managed_date_factory_survives_clock_reentry_and_releases_its_cycle() {
        let runtime = crate::runtime::Runtime::new();
        let constructor = Object::new_in(&runtime);
        runtime.collect_migration_graph().unwrap();
        let prototype = Object::new_in(&runtime);
        let weak = prototype.downgrade();
        let calls = Rc::new(std::cell::Cell::new(0));
        let callback_calls = calls.clone();
        let callback_runtime = runtime.clone();
        let host = HostHooks::new(move || {
            callback_runtime.collect_migration_graph().unwrap();
            callback_calls.set(callback_calls.get() + 1);
            1234.9
        });
        DateFactory::install(&constructor, &prototype, host).unwrap();
        prototype
            .create_data_property("constructor".into(), Value::Object(constructor.clone()))
            .unwrap();
        drop(prototype);
        runtime.verify_migration_barriers().unwrap();
        assert_eq!(runtime.diagnostics().rooted, 1);
        assert!(
            constructor
                .call(Value::Undefined, &[])
                .unwrap()
                .same_value(&Value::String("1970-01-01T00:00:01.234Z".into()))
        );
        assert_eq!(calls.get(), 1);
        let date =
            DateObject::from_object(constructor.construct(&[], constructor.clone()).unwrap())
                .unwrap();
        assert_eq!(date.value(), 1234.0);
        assert_eq!(date.object().prototype(), weak.upgrade());
        let explicit = DateObject::from_object(
            constructor
                .construct(&[Value::Number(42.0)], constructor.clone())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(explicit.value(), 42.0);
        assert_eq!(calls.get(), 2);
        drop(explicit);
        drop(constructor);
        runtime.collect_migration_graph().unwrap();
        assert_eq!(
            date.to_iso_string().unwrap(),
            JsString::from("1970-01-01T00:00:01.234Z")
        );
        drop(date);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }
}
