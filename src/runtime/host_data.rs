//! Date, JSON, and explicit host/Intl boundaries.

use std::{
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

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

impl DateObject {
    /// Creates a Date after applying the ECMAScript TimeClip boundary.
    #[must_use]
    pub fn new(milliseconds: f64) -> Self {
        let value = if milliseconds.is_finite() && milliseconds.abs() <= 8.64e15 {
            milliseconds.trunc()
        } else {
            f64::NAN
        };
        let object = Object::new();
        object.0.state.borrow_mut().date_value = Some(value);
        Self(object)
    }
    /// Creates a Date using an explicit host clock.
    #[must_use]
    pub fn now(host: &HostHooks) -> Self {
        Self::new(host.now())
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
        let value = self.value();
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

/// Callback shape used by JSON reviver and replacer operations.
pub type JsonTransform<'a> = dyn FnMut(&str, Value) -> RuntimeResult<Value> + 'a;

impl Json {
    /// Parses JSON and optionally walks it bottom-up with a reviver.
    pub fn parse(
        source: &JsString,
        mut reviver: Option<&mut JsonTransform<'_>>,
    ) -> RuntimeResult<Value> {
        let parsed: serde_json::Value = serde_json::from_str(&source.to_string_lossy())
            .map_err(|error| RuntimeError::syntax_error(error.to_string()))?;
        let value = from_json(parsed)?;
        if let Some(reviver) = reviver.as_mut() {
            revive("", value, *reviver)
        } else {
            Ok(value)
        }
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

fn from_json(value: serde_json::Value) -> RuntimeResult<Value> {
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
            let array = ArrayObject::new(values.len() as u32)?;
            for (index, value) in values.into_iter().enumerate() {
                array.object().create_data_property(
                    JsString::from(index.to_string()).into(),
                    from_json(value)?,
                )?;
            }
            Value::Object(array.object())
        }
        serde_json::Value::Object(values) => {
            let object = Object::new();
            for (key, value) in values {
                object.create_data_property(JsString::from(key).into(), from_json(value)?)?;
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
        for property in object.own_property_keys() {
            let PropertyKey::String(name) = property else {
                continue;
            };
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
