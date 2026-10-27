//! Generic Array stack/queue algorithms. Keep observable Get/Has/Set/Delete
//! operations ordered; indexed storage can optimize those internal methods.

use std::rc::Rc;

use super::{
    BigIntObject, IntrinsicId, IntrinsicRegistry, JsString, NumberObject, Object,
    PropertyDescriptor, PropertyKey, RuntimeError, RuntimeResult, StringObject, Value,
    WellKnownSymbol, WellKnownSymbols,
};

const MAX_LENGTH: u64 = 9_007_199_254_740_991;

struct Coercion {
    string: Object,
    number: Object,
    boolean: Object,
    symbol: Object,
    bigint: Object,
    to_primitive: PropertyKey,
    to_string_tag: PropertyKey,
}

impl Coercion {
    fn object(&self, value: Value) -> RuntimeResult<Object> {
        let (object, prototype) = match value {
            Value::Object(object) => return Ok(object),
            Value::Undefined | Value::Null => {
                return Err(RuntimeError::type_error(
                    "array method receiver is null or undefined",
                ));
            }
            Value::String(value) => (StringObject::new(value).object(), &self.string),
            Value::Number(value) => (NumberObject::new(value).object(), &self.number),
            Value::BigInt(value) => (BigIntObject::new(value).object(), &self.bigint),
            value @ Value::Boolean(_) | value @ Value::Symbol(_) => {
                let prototype = if matches!(value, Value::Boolean(_)) {
                    &self.boolean
                } else {
                    &self.symbol
                };
                let object = Object::new();
                // The primitive slot is also used by the Number/BigInt wrappers.
                object.0.state.borrow_mut().primitive_data = Some(value);
                (object, prototype)
            }
        };
        object.set_prototype(Some(prototype.clone()));
        Ok(object)
    }

    fn number(&self, value: Value) -> RuntimeResult<f64> {
        super::coercion::primitive(&value, &self.to_primitive, false)?.to_number()
    }

    fn length(&self, object: &Object) -> RuntimeResult<u64> {
        let key: PropertyKey = "length".into();
        let value = match object.own_data_value_with_hint(&key, None) {
            Some((value, _)) => value,
            None => object.get(&key)?,
        };
        let number = match value {
            Value::Number(number) => number,
            value => self.number(value)?,
        };
        // Rust's saturating float cast maps NaN and negative values to zero.
        // Explicit clamping implements ToLength for positive infinity as well.
        Ok((number as u64).min(MAX_LENGTH))
    }
}

pub(super) fn install(
    intrinsics: &IntrinsicRegistry,
    symbols: &WellKnownSymbols,
) -> RuntimeResult<()> {
    let coercion = Rc::new(Coercion {
        string: intrinsics.get(IntrinsicId::StringPrototype),
        number: intrinsics.get(IntrinsicId::NumberPrototype),
        boolean: intrinsics.get(IntrinsicId::BooleanPrototype),
        symbol: intrinsics.get(IntrinsicId::SymbolPrototype),
        bigint: intrinsics.get(IntrinsicId::BigIntPrototype),
        to_primitive: symbols.get(WellKnownSymbol::ToPrimitive).into(),
        to_string_tag: symbols.get(WellKnownSymbol::ToStringTag).into(),
    });
    let function_prototype = intrinsics.get(IntrinsicId::FunctionPrototype);
    let object_prototype = intrinsics.get(IntrinsicId::ObjectPrototype);
    let boxing = coercion.clone();
    method(
        &object_prototype,
        &function_prototype,
        "valueOf",
        0,
        move |this, _| boxing.object(this).map(Value::Object),
    )?;
    let boxing = coercion.clone();
    method(
        &object_prototype,
        &function_prototype,
        "toString",
        0,
        move |this, _| {
            match this {
                Value::Undefined => return Ok(Value::String("[object Undefined]".into())),
                Value::Null => return Ok(Value::String("[object Null]".into())),
                _ => {}
            };
            let object = boxing.object(this)?;
            let tag = object_tag(&object)?;
            let tag = match object.get(&boxing.to_string_tag)? {
                Value::String(tag) => tag,
                _ => tag.into(),
            };
            let mut units = JsString::from("[object ").code_units().to_vec();
            units.extend_from_slice(tag.code_units());
            units.push(u16::from(b']'));
            Ok(Value::String(JsString::from_code_units(units)))
        },
    )?;

    let prototype = intrinsics.get(IntrinsicId::ArrayPrototype);
    for (name, arity, operation) in [
        ("push", 1, push as Operation),
        ("pop", 0, pop as Operation),
        ("shift", 0, shift as Operation),
        ("unshift", 1, unshift as Operation),
    ] {
        let coercion = coercion.clone();
        method(
            &prototype,
            &function_prototype,
            name,
            arity,
            move |this, items| {
                let object = coercion.object(this)?;
                let length = coercion.length(&object)?;
                operation(&object, length, items)
            },
        )?;
    }
    Ok(())
}

type Operation = fn(&Object, u64, &[Value]) -> RuntimeResult<Value>;

pub(super) fn method(
    holder: &Object,
    prototype: &Object,
    name: &str,
    length: u32,
    callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
) -> RuntimeResult<()> {
    let function = super::native_function::create(prototype, name, length, callback)?;
    holder.define_own_property(
        name.into(),
        PropertyDescriptor::data(Value::Object(function), true, false, true),
    )?;
    Ok(())
}

fn key(mut index: u64) -> PropertyKey {
    // Array algorithms use nonnegative integer keys, including generic
    // array-like indices beyond u32::MAX. Write decimal UTF-16 directly so
    // short keys use JsString's inline storage without a temporary String.
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

fn set(object: &Object, key: PropertyKey, value: Value) -> RuntimeResult<()> {
    if !object.set(&key, value)? {
        return Err(RuntimeError::type_error(
            "array method could not set property",
        ));
    }
    Ok(())
}

fn length(object: &Object, length: u64) -> RuntimeResult<Value> {
    let value = Value::Number(length as f64);
    set(object, "length".into(), value.clone())?;
    Ok(value)
}

fn delete(object: &Object, index: u64) -> RuntimeResult<()> {
    if !object.try_delete(&key(index))? {
        return Err(RuntimeError::type_error(
            "array method could not delete property",
        ));
    }
    Ok(())
}

fn push(object: &Object, len: u64, items: &[Value]) -> RuntimeResult<Value> {
    let count = items.len() as u64;
    if count > MAX_LENGTH - len {
        return Err(RuntimeError::type_error(
            "array-like length exceeds the safe integer limit",
        ));
    }
    if let Some(end) = object.try_push_array_values(len, items) {
        return Ok(Value::Number(f64::from(end)));
    }
    for (index, item) in items.iter().enumerate() {
        set(object, key(len + index as u64), item.clone())?;
    }
    length(object, len + count)
}

fn pop(object: &Object, len: u64, _: &[Value]) -> RuntimeResult<Value> {
    if len == 0 {
        length(object, 0)?;
        return Ok(Value::Undefined);
    }
    let value = object.get(&key(len - 1))?;
    delete(object, len - 1)?;
    length(object, len - 1)?;
    Ok(value)
}

fn shift(object: &Object, len: u64, _: &[Value]) -> RuntimeResult<Value> {
    if len == 0 {
        length(object, 0)?;
        return Ok(Value::Undefined);
    }
    let value = object.get(&key(0))?;
    for from in 1..len {
        move_property(object, from, from - 1)?;
    }
    delete(object, len - 1)?;
    length(object, len - 1)?;
    Ok(value)
}

fn unshift(object: &Object, len: u64, items: &[Value]) -> RuntimeResult<Value> {
    let count = items.len() as u64;
    if count > MAX_LENGTH - len {
        return Err(RuntimeError::type_error(
            "array-like length exceeds the safe integer limit",
        ));
    }
    if count != 0 {
        for from in (0..len).rev() {
            move_property(object, from, from + count)?;
        }
        for (index, item) in items.iter().enumerate() {
            set(object, key(index as u64), item.clone())?;
        }
    }
    length(object, len + count)
}

fn move_property(object: &Object, from: u64, to: u64) -> RuntimeResult<()> {
    let from = key(from);
    if object.has_property(&from)? {
        set(object, key(to), object.get(&from)?)
    } else {
        delete(object, to)
    }
}

fn is_array(object: &Object) -> RuntimeResult<bool> {
    let mut object = object.clone();
    loop {
        let state = object.0.state.borrow();
        if state.array {
            return Ok(true);
        }
        let proxy = state.proxy.clone();
        drop(state);
        let Some(proxy) = proxy else {
            return Ok(false);
        };
        object = proxy
            .borrow()
            .as_ref()
            .ok_or_else(|| RuntimeError::type_error("revoked Proxy"))?
            .0
            .clone();
    }
}

fn object_tag(object: &Object) -> RuntimeResult<&'static str> {
    if is_array(object)? {
        return Ok("Array");
    }
    if object.is_callable() {
        return Ok("Function");
    }
    let state = object.0.state.borrow();
    Ok(if state.string_data.is_some() {
        "String"
    } else if state.date_value.is_some() {
        "Date"
    } else if state.regexp.is_some() {
        "RegExp"
    } else {
        match state.primitive_data {
            Some(Value::Number(_)) => "Number",
            Some(Value::Boolean(_)) => "Boolean",
            _ => "Object",
        }
    })
}

// Preserve ordinary prototype lookup when boxing primitive generic receivers.
pub(super) fn iterator_boxing(
    intrinsics: &IntrinsicRegistry,
    symbols: &WellKnownSymbols,
) -> impl Fn(Value) -> RuntimeResult<Object> + 'static {
    let coercion = Coercion {
        string: intrinsics.get(IntrinsicId::StringPrototype),
        number: intrinsics.get(IntrinsicId::NumberPrototype),
        boolean: intrinsics.get(IntrinsicId::BooleanPrototype),
        symbol: intrinsics.get(IntrinsicId::SymbolPrototype),
        bigint: intrinsics.get(IntrinsicId::BigIntPrototype),
        to_primitive: symbols.get(WellKnownSymbol::ToPrimitive).into(),
        to_string_tag: symbols.get(WellKnownSymbol::ToStringTag).into(),
    };
    move |value| coercion.object(value)
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn integer_keys_preserve_decimal_boundaries_without_number_rounding() {
        for (index, expected) in [
            (0, "0"),
            (9, "9"),
            (10, "10"),
            (u64::from(u32::MAX), "4294967295"),
            (MAX_LENGTH, "9007199254740991"),
            (u64::MAX, "18446744073709551615"),
        ] {
            assert_eq!(key(index), PropertyKey::from(expected));
        }
        let mut power = 10u64;
        loop {
            for index in [power - 1, power, power + 1] {
                assert_eq!(
                    key(index),
                    PropertyKey::from(JsString::from(index.to_string()))
                );
            }
            let Some(next) = power.checked_mul(10) else {
                break;
            };
            power = next;
        }
    }
}
