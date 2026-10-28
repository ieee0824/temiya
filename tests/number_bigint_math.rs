use num_bigint::BigInt;
use temiya::runtime::{
    IntrinsicId, JsString, Math, NumberObject, Realm, RuntimeErrorKind, Value, parse_float,
    parse_int,
};

fn method(holder: &temiya::runtime::Object, name: &str) -> temiya::runtime::Object {
    let Value::Object(method) = holder.get(&name.into()).unwrap() else {
        panic!()
    };
    method
}

#[test]
fn number_constructor_predicates_and_wrapper_preserve_edges() {
    let context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&context);
    let number = realm.intrinsic(IntrinsicId::Number);
    assert!(matches!(
        number.call(Value::Undefined, &[]).unwrap(),
        Value::Number(0.0)
    ));
    let boxed = number
        .construct(&[Value::Number(-0.0)], number.clone())
        .unwrap();
    let wrapper = NumberObject::new(&context, -0.0);
    assert!(wrapper.value().is_sign_negative());
    let value = method(&realm.intrinsic(IntrinsicId::NumberPrototype), "valueOf")
        .call(Value::Object(boxed), &[])
        .unwrap();
    assert!(matches!(value, Value::Number(value) if value == 0.0 && value.is_sign_negative()));
    assert!(matches!(
        method(&number, "isNaN")
            .call(Value::Undefined, &[Value::Number(f64::NAN)])
            .unwrap(),
        Value::Boolean(true)
    ));
    assert!(matches!(
        method(&number, "isFinite")
            .call(Value::Undefined, &[Value::Number(f64::INFINITY)])
            .unwrap(),
        Value::Boolean(false)
    ));
}

#[test]
fn parsing_accepts_prefixes_and_radix_boundaries() {
    assert_eq!(parse_float(&JsString::from("  -1.25e2tail")), -125.0);
    assert_eq!(parse_int(&JsString::from("0xffrest"), None), 255.0);
    assert_eq!(parse_int(&JsString::from("1012"), Some(2)), 5.0);
    assert!(parse_int(&JsString::from("10"), Some(1)).is_nan());
    assert!(parse_float(&JsString::from("nope")).is_nan());
}

#[test]
fn math_handles_nan_signed_zero_infinity_and_rounding() {
    assert!(Math::max(&[1.0, f64::NAN]).is_nan());
    assert!(Math::max(&[-0.0, 0.0]).is_sign_positive());
    assert!(Math::min(&[0.0, -0.0]).is_sign_negative());
    assert!(Math::round(-0.1).is_sign_negative());
    assert_eq!(Math::round(1.5), 2.0);
    assert_eq!(Math::max(&[]), f64::NEG_INFINITY);
    assert_eq!(Math::min(&[]), f64::INFINITY);
}

#[test]
fn bigint_width_operations_and_mixed_numeric_errors() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    assert_eq!(
        Math::bigint_as_uint_n(8, &BigInt::from(-1)).unwrap(),
        BigInt::from(255)
    );
    assert_eq!(
        Math::bigint_as_int_n(8, &BigInt::from(255)).unwrap(),
        BigInt::from(-1)
    );
    let realm = Realm::ordinary_in(&allocation_context);
    let bigint = realm.intrinsic(IntrinsicId::BigInt);
    assert!(!bigint.is_constructor());
    assert!(matches!(
        bigint.call(Value::Undefined, &[Value::String("42".into())]).unwrap(),
        Value::BigInt(value) if value == BigInt::from(42)
    ));
    let evaluator = temiya::runtime::ExpressionEvaluator::new(realm);
    let expression = temiya::parser::parse_expression("1n + 1").unwrap();
    assert_eq!(
        evaluator.evaluate(&expression).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn math_intrinsic_converts_arguments_and_rejects_bigint() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&allocation_context);
    let math = realm.intrinsic(IntrinsicId::Math);
    assert!(matches!(
        method(&math, "round")
            .call(Value::Undefined, &[Value::Number(2.5)])
            .unwrap(),
        Value::Number(3.0)
    ));
    assert_eq!(
        method(&math, "abs")
            .call(Value::Undefined, &[Value::BigInt(BigInt::from(1))])
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
}
