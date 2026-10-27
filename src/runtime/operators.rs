//! Shared binary coercion, comparisons and instanceof operations.
use super::{
    Numeric, Object, PrimitiveHint, PropertyKey, Realm, RuntimeError, RuntimeResult, Value,
    WellKnownSymbol,
};
use num_bigint::{BigInt, Sign};
use std::cmp::Ordering;

fn primitive(realm: Option<&Realm>, value: Value, hint: PrimitiveHint) -> RuntimeResult<Value> {
    if !matches!(value, Value::Object(_)) {
        return Ok(value);
    }
    if let Some(realm) = realm {
        super::coercion::primitive_with_hint(
            &value,
            &realm.well_known_symbol(WellKnownSymbol::ToPrimitive).into(),
            hint,
        )
    } else {
        value.to_primitive(hint)
    }
}

fn numeric(realm: Option<&Realm>, value: Value) -> RuntimeResult<Numeric> {
    match primitive(realm, value, PrimitiveHint::Number)? {
        Value::BigInt(value) => Ok(Numeric::BigInt(value)),
        value => value.to_number().map(Numeric::Number),
    }
}

#[cold]
#[inline(never)]
pub(super) fn object_binary(
    realm: &Realm,
    operator: &str,
    left: Value,
    right: Value,
) -> RuntimeResult<Value> {
    match operator {
        "+" => {
            let left = primitive(Some(realm), left, PrimitiveHint::Default)?;
            let right = primitive(Some(realm), right, PrimitiveHint::Default)?;
            super::interpreter::binary_values(operator, left, right)
        }
        "-" | "*" | "/" | "%" | "**" | "&" | "|" | "^" | "<<" | ">>" | ">>>" => {
            let left = numeric(Some(realm), left)?;
            let right = numeric(Some(realm), right)?;
            super::interpreter::numeric_binary_result(operator, left, right)
        }
        "==" | "!=" => {
            equal(Some(realm), left, right).map(|value| Value::Boolean(value == (operator == "==")))
        }
        "<" | ">" | "<=" | ">=" => relational(Some(realm), operator, left, right),
        "in" => {
            let Value::Object(object) = right else {
                return Err(RuntimeError::type_error(
                    "right-hand side of 'in' is not an object",
                ));
            };
            let key = primitive(Some(realm), left, PrimitiveHint::String)?.to_property_key()?;
            object.has_property(&key).map(Value::Boolean)
        }
        "instanceof" => instance_of(
            left,
            right,
            Some(realm.well_known_symbol(WellKnownSymbol::HasInstance).into()),
        )
        .map(Value::Boolean),
        _ => super::interpreter::binary_values(operator, left, right),
    }
}

pub(super) fn equal(realm: Option<&Realm>, left: Value, right: Value) -> RuntimeResult<bool> {
    if std::mem::discriminant(&left) == std::mem::discriminant(&right) {
        return Ok(super::interpreter::strict_equal(&left, &right));
    }
    match (left, right) {
        (Value::Null, Value::Undefined) | (Value::Undefined, Value::Null) => Ok(true),
        (Value::Number(a), Value::String(b)) | (Value::String(b), Value::Number(a)) => {
            Ok(a == Value::String(b).to_number()?)
        }
        (Value::Boolean(a), b) => equal(realm, Value::Number(if a { 1.0 } else { 0.0 }), b),
        (a, Value::Boolean(b)) => equal(realm, a, Value::Number(if b { 1.0 } else { 0.0 })),
        (
            a @ Value::Object(_),
            b @ (Value::String(_) | Value::Number(_) | Value::BigInt(_) | Value::Symbol(_)),
        ) => equal(realm, primitive(realm, a, PrimitiveHint::Default)?, b),
        (
            a @ (Value::String(_) | Value::Number(_) | Value::BigInt(_) | Value::Symbol(_)),
            b @ Value::Object(_),
        ) => equal(realm, a, primitive(realm, b, PrimitiveHint::Default)?),
        (Value::BigInt(a), Value::String(b)) | (Value::String(b), Value::BigInt(a)) => {
            Ok(Value::String(b).to_bigint().is_ok_and(|b| a == b))
        }
        (Value::BigInt(a), Value::Number(b)) | (Value::Number(b), Value::BigInt(a)) => {
            Ok(compare_bigint_number(&a, b) == Some(Ordering::Equal))
        }
        _ => Ok(false),
    }
}

// Compare a mathematical integer with the exact IEEE-754 value, without decimal
// formatting, lossy BigInt->Number conversion, or a new numeric dependency.
fn compare_bigint_number(integer: &BigInt, number: f64) -> Option<Ordering> {
    if number.is_nan() {
        return None;
    }
    if number == f64::INFINITY {
        return Some(Ordering::Less);
    }
    if number == f64::NEG_INFINITY {
        return Some(Ordering::Greater);
    }
    let bits = number.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let significand = if exponent == 0 {
        fraction
    } else {
        fraction | (1_u64 << 52)
    };
    let shift = if exponent == 0 {
        -1074
    } else {
        exponent - 1023 - 52
    };
    let mut significand = BigInt::from(significand);
    if bits >> 63 != 0 {
        significand = -significand;
    }
    Some(if shift >= 0 {
        integer.cmp(&(significand << shift as usize))
    } else {
        (integer << (-shift) as usize).cmp(&significand)
    })
}

pub(super) fn relational(
    realm: Option<&Realm>,
    operator: &str,
    left: Value,
    right: Value,
) -> RuntimeResult<Value> {
    let left = primitive(realm, left, PrimitiveHint::Number)?;
    let right = primitive(realm, right, PrimitiveHint::Number)?;
    let order = match (&left, &right) {
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (Value::BigInt(a), Value::String(b)) => {
            Value::String(b.clone()).to_bigint().ok().map(|b| a.cmp(&b))
        }
        (Value::String(a), Value::BigInt(b)) => {
            Value::String(a.clone()).to_bigint().ok().map(|a| a.cmp(b))
        }
        _ => match (numeric(None, left)?, numeric(None, right)?) {
            (Numeric::Number(a), Numeric::Number(b)) => a.partial_cmp(&b),
            (Numeric::BigInt(a), Numeric::BigInt(b)) => Some(a.cmp(&b)),
            (Numeric::BigInt(a), Numeric::Number(b)) => compare_bigint_number(&a, b),
            (Numeric::Number(a), Numeric::BigInt(b)) => {
                compare_bigint_number(&b, a).map(Ordering::reverse)
            }
        },
    };
    Ok(Value::Boolean(match operator {
        "<" => order == Some(Ordering::Less),
        ">" => order == Some(Ordering::Greater),
        "<=" => matches!(order, Some(Ordering::Less | Ordering::Equal)),
        ">=" => matches!(order, Some(Ordering::Greater | Ordering::Equal)),
        _ => return Err(RuntimeError::internal_error("invalid relational operator")),
    }))
}

pub(super) fn number_power(base: f64, exponent: f64) -> f64 {
    if exponent == 0.0 {
        return 1.0;
    }
    if base.abs() == 1.0 && exponent.is_infinite() {
        return f64::NAN;
    }
    base.powf(exponent)
}

pub(super) fn right_shift(value: BigInt, count: BigInt) -> RuntimeResult<BigInt> {
    if count.sign() == Sign::Minus {
        return Err(RuntimeError::internal_error("negative right shift count"));
    }
    if count >= BigInt::from(value.bits()) {
        return Ok(if value.sign() == Sign::Minus {
            BigInt::from(-1)
        } else {
            BigInt::from(0)
        });
    }
    let count = count
        .to_string()
        .parse::<usize>()
        .map_err(|_| RuntimeError::range_error("BigInt shift count too large"))?;
    Ok(value >> count)
}

pub(super) fn instance_of(
    left: Value,
    right: Value,
    key: Option<PropertyKey>,
) -> RuntimeResult<bool> {
    let Value::Object(constructor) = right else {
        return Err(RuntimeError::type_error(
            "right-hand side of instanceof is not an object",
        ));
    };
    has_instance(left, constructor, key.as_ref(), false)
}

pub(super) fn ordinary_has_instance(
    constructor: Value,
    left: Value,
    key: &PropertyKey,
) -> RuntimeResult<bool> {
    let Value::Object(constructor) = constructor else {
        return Ok(false);
    };
    has_instance(left, constructor, Some(key), true)
}

fn has_instance(
    left: Value,
    mut constructor: Object,
    key: Option<&PropertyKey>,
    mut ordinary: bool,
) -> RuntimeResult<bool> {
    loop {
        if !ordinary {
            if let Some(key) = key {
                let method = constructor.get(key)?;
                if !matches!(method, Value::Undefined | Value::Null) {
                    let Value::Object(method) = method else {
                        return Err(RuntimeError::type_error("@@hasInstance is not callable"));
                    };
                    return method
                        .call(Value::Object(constructor), &[left])
                        .map(|value| value.to_boolean());
                }
            }
            if !constructor.is_callable() {
                return Err(RuntimeError::type_error(
                    "instanceof target is not callable",
                ));
            }
        } else if !constructor.is_callable() {
            return Ok(false);
        }
        let bound = constructor.0.state.borrow().bound_target.clone();
        if let Some(target) = bound {
            constructor = target;
            ordinary = false;
            continue;
        }
        let Value::Object(mut object) = left else {
            return Ok(false);
        };
        let Value::Object(prototype) = constructor.get(&"prototype".into())? else {
            return Err(RuntimeError::type_error(
                "constructor prototype is not an object",
            ));
        };
        loop {
            let Some(next) = object.try_prototype()? else {
                return Ok(false);
            };
            if next == prototype {
                return Ok(true);
            }
            object = next;
        }
    }
}
