//! Number and BigInt wrappers, parsing helpers, and deterministic Math operations.

use num_bigint::{BigInt, Sign};

use super::{JsString, Object, RuntimeError, RuntimeResult, Value};

/// A boxed Number primitive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumberObject(Object);

impl NumberObject {
    /// Boxes a Number value.
    #[must_use]
    pub fn new(context: &super::AllocationContext, value: f64) -> Self {
        let object = Object::new_in(context);
        object.0.state.borrow_mut().primitive_data = Some(Box::new(Value::Number(value)));
        Self(object)
    }

    pub(crate) fn new_for(context: &Object, value: f64) -> Self {
        let object = Object::new_for(context);
        object.0.state.borrow_mut().primitive_data = Some(Box::new(Value::Number(value)));
        Self(object)
    }

    /// Returns the underlying JavaScript object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }

    /// Returns the boxed Number.
    #[must_use]
    pub fn value(&self) -> f64 {
        match self.0.0.state.borrow().primitive_data.as_deref() {
            Some(Value::Number(value)) => *value,
            _ => unreachable!("Number data"),
        }
    }
}

/// A boxed BigInt primitive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BigIntObject(Object);

impl BigIntObject {
    /// Boxes a BigInt value.
    #[must_use]
    pub fn new(context: &super::AllocationContext, value: BigInt) -> Self {
        let object = Object::new_in(context);
        object.0.state.borrow_mut().primitive_data = Some(Box::new(Value::BigInt(value)));
        Self(object)
    }

    pub(crate) fn new_for(context: &Object, value: BigInt) -> Self {
        let object = Object::new_for(context);
        object.0.state.borrow_mut().primitive_data = Some(Box::new(Value::BigInt(value)));
        Self(object)
    }

    /// Returns the underlying JavaScript object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }

    /// Returns the boxed BigInt.
    #[must_use]
    pub fn value(&self) -> BigInt {
        match self.0.0.state.borrow().primitive_data.as_deref() {
            Some(Value::BigInt(value)) => value.clone(),
            _ => unreachable!("BigInt data"),
        }
    }
}

/// Parses the longest decimal floating-point prefix, as `parseFloat` does.
#[must_use]
pub fn parse_float(input: &JsString) -> f64 {
    let text = input.to_string_lossy();
    let text = text.trim_start_matches(char::is_whitespace);
    if text.starts_with("Infinity") || text.starts_with("+Infinity") {
        return f64::INFINITY;
    }
    if text.starts_with("-Infinity") {
        return f64::NEG_INFINITY;
    }
    let bytes = text.as_bytes();
    let mut cursor = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let integer_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let mut digits = cursor - integer_start;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let fraction_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        digits += cursor - fraction_start;
    }
    if digits == 0 {
        return f64::NAN;
    }
    let mantissa_end = cursor;
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        let exponent_mark = cursor;
        cursor += 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let exponent_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == exponent_start {
            cursor = exponent_mark;
        }
    }
    text[..cursor.max(mantissa_end)].parse().unwrap_or(f64::NAN)
}

/// Parses an integer prefix with an optional radix from 2 through 36.
#[must_use]
pub fn parse_int(input: &JsString, radix: Option<u32>) -> f64 {
    let mut text = input.to_string_lossy();
    text = text.trim_start_matches(char::is_whitespace).to_owned();
    let (negative, mut digits) = if let Some(rest) = text.strip_prefix('-') {
        (true, rest)
    } else {
        (false, text.strip_prefix('+').unwrap_or(&text))
    };
    let mut radix = radix.unwrap_or(0);
    if radix != 0 && !(2..=36).contains(&radix) {
        return f64::NAN;
    }
    if radix == 0 {
        if digits.starts_with("0x") || digits.starts_with("0X") {
            radix = 16;
            digits = &digits[2..];
        } else {
            radix = 10;
        }
    } else if radix == 16 && (digits.starts_with("0x") || digits.starts_with("0X")) {
        digits = &digits[2..];
    }
    let prefix = digits
        .chars()
        .take_while(|value| value.is_digit(radix))
        .collect::<String>();
    if prefix.is_empty() {
        return f64::NAN;
    }
    let value = BigInt::parse_bytes(prefix.as_bytes(), radix).expect("validated digits");
    let value = value.to_string().parse::<f64>().unwrap_or(f64::INFINITY);
    if negative { -value } else { value }
}

/// ECMAScript Math operations whose edge behavior differs from direct Rust methods.
pub struct Math;

impl Math {
    /// Rounds toward positive infinity at a half, preserving negative zero.
    #[must_use]
    pub fn round(value: f64) -> f64 {
        if value.is_nan() || value.is_infinite() || value == 0.0 {
            return value;
        }
        let rounded = (value + 0.5).floor();
        if rounded == 0.0 && value.is_sign_negative() {
            -0.0
        } else {
            rounded
        }
    }

    /// Returns the maximum, propagating NaN and preferring positive zero.
    #[must_use]
    pub fn max(values: &[f64]) -> f64 {
        values
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, |result, value| {
                if result.is_nan() || value.is_nan() {
                    f64::NAN
                } else if value > result
                    || (value == 0.0 && result == 0.0 && value.is_sign_positive())
                {
                    value
                } else {
                    result
                }
            })
    }

    /// Returns the minimum, propagating NaN and preferring negative zero.
    #[must_use]
    pub fn min(values: &[f64]) -> f64 {
        values.iter().copied().fold(f64::INFINITY, |result, value| {
            if result.is_nan() || value.is_nan() {
                f64::NAN
            } else if value < result || (value == 0.0 && result == 0.0 && value.is_sign_negative())
            {
                value
            } else {
                result
            }
        })
    }

    /// Implements `BigInt.asUintN`.
    pub fn bigint_as_uint_n(bits: u64, value: &BigInt) -> RuntimeResult<BigInt> {
        let modulus = modulus(bits)?;
        let result = ((value % &modulus) + &modulus) % &modulus;
        Ok(result)
    }

    /// Implements `BigInt.asIntN`.
    pub fn bigint_as_int_n(bits: u64, value: &BigInt) -> RuntimeResult<BigInt> {
        if bits == 0 {
            return Ok(BigInt::from(0));
        }
        let modulus = modulus(bits)?;
        let unsigned = Self::bigint_as_uint_n(bits, value)?;
        let boundary = &modulus >> 1;
        Ok(if unsigned >= boundary {
            unsigned - modulus
        } else {
            unsigned
        })
    }
}

fn modulus(bits: u64) -> RuntimeResult<BigInt> {
    let shift =
        usize::try_from(bits).map_err(|_| RuntimeError::range_error("bit count is too large"))?;
    if shift > 1_000_000 {
        return Err(RuntimeError::range_error("bit count is too large"));
    }
    Ok(BigInt::from_biguint(
        Sign::Plus,
        num_bigint::BigUint::from(1_u8) << shift,
    ))
}

// NumberToBigInt reads the exact binary64 integer, not its shortest decimal
// spelling (which can denote a different mathematical integer above 2^53).
pub(super) fn number_to_bigint(value: f64) -> RuntimeResult<BigInt> {
    if !value.is_finite() || value.fract() != 0.0 {
        return Err(RuntimeError::range_error("Number is not a finite integer"));
    }
    if value == 0.0 {
        return Ok(BigInt::from(0));
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023 - 52;
    let significand = BigInt::from((bits & ((1_u64 << 52) - 1)) | (1_u64 << 52));
    let result = if exponent >= 0 {
        significand << exponent as usize
    } else {
        significand >> (-exponent) as usize
    };
    Ok(if bits >> 63 == 0 { result } else { -result })
}
