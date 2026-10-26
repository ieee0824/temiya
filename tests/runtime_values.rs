use std::cell::RefCell;
use std::rc::Rc;

use num_bigint::BigInt;
use temiya::runtime::{
    JsString, Numeric, Object, PrimitiveHint, RuntimeError, RuntimeErrorKind, Symbol, Value,
};

fn string(value: &str) -> Value {
    Value::String(JsString::from(value))
}

#[test]
fn values_preserve_utf16_and_identity() {
    let lone_surrogate = JsString::from_code_units(vec![0xd800, b'a' as u16]);
    assert_eq!(lone_surrogate.code_units(), &[0xd800, 0x61]);

    let first = Symbol::new(Some(JsString::from("key")));
    let first_clone = first.clone();
    let second = Symbol::new(Some(JsString::from("key")));
    assert!(Value::Symbol(first).same_value(&Value::Symbol(first_clone)));
    assert!(
        !Value::Symbol(second.clone())
            .same_value(&Value::Symbol(Symbol::new(second.description().cloned())))
    );

    let object = Object::new();
    assert!(Value::Object(object.clone()).same_value(&Value::Object(object)));
    assert!(!Value::Object(Object::new()).same_value(&Value::Object(Object::new())));
}

#[test]
fn same_value_handles_nan_and_signed_zero() {
    assert!(Value::Number(f64::NAN).same_value(&Value::Number(f64::NAN)));
    assert!(!Value::Number(0.0).same_value(&Value::Number(-0.0)));
    assert!(Value::Number(0.0).same_value_zero(&Value::Number(-0.0)));
    assert!(!Value::Number(1.0).same_value(&Value::BigInt(BigInt::from(1))));
}

#[test]
fn to_boolean_follows_ecmascript_falsiness() {
    for value in [
        Value::Undefined,
        Value::Null,
        Value::Boolean(false),
        Value::Number(0.0),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        string(""),
        Value::BigInt(BigInt::from(0)),
    ] {
        assert!(!value.to_boolean(), "{value:?}");
    }
    assert!(Value::Object(Object::new()).to_boolean());
    assert!(Value::Symbol(Symbol::new(None)).to_boolean());
}

#[test]
fn to_primitive_observes_exotic_and_ordinary_order() {
    let object = Object::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let value_calls = Rc::clone(&calls);
    object.set_value_of_hook(move || {
        value_calls.borrow_mut().push("valueOf");
        Ok(Value::Object(Object::new()))
    });
    let string_calls = Rc::clone(&calls);
    object.set_to_string_hook(move || {
        string_calls.borrow_mut().push("toString");
        Ok(string("converted"))
    });
    assert!(matches!(
        Value::Object(object).to_primitive(PrimitiveHint::Number),
        Ok(Value::String(value)) if value == JsString::from("converted")
    ));
    assert_eq!(calls.borrow().as_slice(), &["valueOf", "toString"]);

    let exotic = Object::new();
    exotic.set_to_primitive_hook(|hint| {
        assert_eq!(hint, PrimitiveHint::String);
        Ok(Value::Number(7.0))
    });
    assert!(matches!(
        Value::Object(exotic).to_primitive(PrimitiveHint::String),
        Ok(Value::Number(7.0))
    ));
}

#[test]
fn to_primitive_propagates_errors_and_rejects_object_results() {
    let throwing = Object::new();
    throwing.set_to_primitive_hook(|_| Err(RuntimeError::type_error("boom")));
    let error = Value::Object(throwing)
        .to_primitive(PrimitiveHint::Default)
        .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::TypeError);
    assert_eq!(error.message(), "boom");

    let invalid = Object::new();
    invalid.set_to_primitive_hook(|_| Ok(Value::Object(Object::new())));
    assert!(
        Value::Object(invalid)
            .to_primitive(PrimitiveHint::Default)
            .is_err()
    );
}

#[test]
fn number_and_numeric_conversions_cover_boundaries() {
    assert_eq!(Value::Null.to_number().unwrap(), 0.0);
    assert!(Value::Undefined.to_number().unwrap().is_nan());
    assert_eq!(string("\u{feff} 0x10\n").to_number().unwrap(), 16.0);
    assert!(string("\u{0085}").to_number().unwrap().is_nan());
    assert_eq!(string("-Infinity").to_number().unwrap(), f64::NEG_INFINITY);
    assert!(string("+0x10").to_number().unwrap().is_nan());
    assert!(string("inf").to_number().unwrap().is_nan());
    assert!(
        Value::String(JsString::from_code_units(vec![0xd800]))
            .to_number()
            .unwrap()
            .is_nan()
    );
    assert!(Value::Symbol(Symbol::new(None)).to_number().is_err());
    assert!(Value::BigInt(BigInt::from(1)).to_number().is_err());

    let huge = BigInt::parse_bytes(b"123456789012345678901234567890", 10).unwrap();
    assert_eq!(
        Value::BigInt(huge.clone()).to_numeric().unwrap(),
        Numeric::BigInt(huge)
    );
    assert_eq!(
        string("1.5e2").to_numeric().unwrap(),
        Numeric::Number(150.0)
    );
}

#[test]
fn string_conversion_uses_javascript_number_formatting() {
    let cases = [
        (f64::NAN, "NaN"),
        (-0.0, "0"),
        (f64::INFINITY, "Infinity"),
        (1e20, "100000000000000000000"),
        (1e21, "1e+21"),
        (1e-6, "0.000001"),
        (1e-7, "1e-7"),
    ];
    for (number, expected) in cases {
        assert_eq!(
            Value::Number(number).to_js_string().unwrap(),
            JsString::from(expected)
        );
    }
    assert_eq!(
        Value::BigInt(BigInt::from(-42)).to_js_string().unwrap(),
        JsString::from("-42")
    );
    assert!(Value::Symbol(Symbol::new(None)).to_js_string().is_err());
}

#[test]
fn bigint_conversion_accepts_only_integer_string_grammar() {
    assert_eq!(string("  -42 ").to_bigint().unwrap(), BigInt::from(-42));
    assert_eq!(string("0xff").to_bigint().unwrap(), BigInt::from(255));
    assert_eq!(string("").to_bigint().unwrap(), BigInt::from(0));
    assert_eq!(Value::Boolean(true).to_bigint().unwrap(), BigInt::from(1));
    assert_eq!(
        string("1.0").to_bigint().unwrap_err().kind(),
        RuntimeErrorKind::SyntaxError
    );
    assert!(string("-0x1").to_bigint().is_err());
    assert!(Value::Number(1.0).to_bigint().is_err());
}
