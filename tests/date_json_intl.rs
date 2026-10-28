use std::{cell::RefCell, rc::Rc};

use temiya::runtime::{
    ArrayObject, DateObject, HostHooks, IntlProvider, JsString, Json, Object, Realm,
    RuntimeErrorKind, UnsupportedIntl, Value,
};

#[test]
fn date_clips_boundaries_and_formats_iso_utc() {
    let context = temiya::runtime::AllocationContext::new();
    assert_eq!(
        DateObject::new(&context, 0.0).to_iso_string().unwrap(),
        JsString::from("1970-01-01T00:00:00.000Z")
    );
    assert_eq!(
        DateObject::new(&context, -1.0).to_iso_string().unwrap(),
        JsString::from("1969-12-31T23:59:59.999Z")
    );
    assert!(DateObject::new(&context, 8.64e15).to_iso_string().is_ok());
    assert_eq!(
        DateObject::new(&context, 8.64e15 + 1.0)
            .to_iso_string()
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::RangeError
    );
}

#[test]
fn realm_date_uses_injected_host_clock() {
    let global = Object::new();
    let realm = Realm::try_new_with_host(
        global.clone(),
        Value::Object(global),
        HostHooks::new(|| 1_234.0),
    )
    .unwrap();
    let date = realm.intrinsic(temiya::runtime::IntrinsicId::Date);
    let Value::Object(now) = date.get(&"now".into()).unwrap() else {
        panic!()
    };
    assert!(matches!(
        now.call(Value::Undefined, &[]).unwrap(),
        Value::Number(1234.0)
    ));
}

#[test]
fn json_round_trip_reviver_and_replacer_observe_property_order() {
    let visited = Rc::new(RefCell::new(Vec::new()));
    let seen = visited.clone();
    let mut reviver = move |key: &str, value: Value| {
        seen.borrow_mut().push(key.to_owned());
        Ok(if key == "drop" {
            Value::Undefined
        } else {
            value
        })
    };
    let parsed = Json::parse(
        &JsString::from(r#"{"keep":1,"drop":2}"#),
        Some(&mut reviver),
    )
    .unwrap();
    let Value::Object(parsed) = parsed else {
        panic!()
    };
    assert!(matches!(
        parsed.get(&"keep".into()).unwrap(),
        Value::Number(1.0)
    ));
    assert!(!parsed.has_property(&"drop".into()).unwrap());
    assert_eq!(visited.borrow().last().map(String::as_str), Some(""));

    let mut replacer = |key: &str, value: Value| {
        Ok(if key == "keep" {
            Value::Number(3.0)
        } else {
            value
        })
    };
    assert_eq!(
        Json::stringify(Value::Object(parsed), Some(&mut replacer)).unwrap(),
        Some(JsString::from(r#"{"keep":3}"#))
    );
}

#[test]
fn json_arrays_map_unsupported_values_to_null_and_reject_cycles() {
    let context = temiya::runtime::AllocationContext::new();
    let array = ArrayObject::new(&context, 3).unwrap();
    array
        .object()
        .create_data_property("1".into(), Value::Number(f64::NAN))
        .unwrap();
    array
        .object()
        .create_data_property("2".into(), Value::Undefined)
        .unwrap();
    assert_eq!(
        Json::stringify(Value::Object(array.object()), None).unwrap(),
        Some(JsString::from("[null,null,null]"))
    );
    let cyclic = Object::new_in(&context);
    cyclic
        .create_data_property("self".into(), Value::Object(cyclic.clone()))
        .unwrap();
    assert_eq!(
        Json::stringify(Value::Object(cyclic), None)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(
        Json::stringify(Value::BigInt(1.into()), None)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn intl_is_an_explicit_unconfigured_host_boundary() {
    assert_eq!(
        UnsupportedIntl
            .format_number("ja-JP", 1.0)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
}
