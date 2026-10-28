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
    let allocation_context = temiya::runtime::AllocationContext::new();
    let global = Object::new_in(&allocation_context);
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
    let context = temiya::runtime::AllocationContext::new();
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
        &context,
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
fn json_context_tree_survives_reviver_collection_and_enters_the_engine() {
    let engine = temiya::Engine::default();
    let realm = engine.create_realm();
    let mut visits = 0;
    let mut reviver = |_: &str, value: Value| {
        visits += 1;
        engine.collect_garbage().unwrap();
        Ok(value)
    };
    let parsed = Json::parse(
        engine.allocation_context(),
        &JsString::from(r#"{"items":[{"answer":1},{"answer":42}],"empty":[]}"#),
        Some(&mut reviver),
    )
    .unwrap();
    assert_eq!(visits, 7);
    let handle = engine.value(&realm, parsed).unwrap();
    engine.set_global(&realm, "host", handle).unwrap();
    engine.collect_garbage().unwrap();
    let script = engine
        .compile(
            &realm,
            "host.items[1].answer===42 && Array.isArray(host.items) && host.empty.length===0;",
        )
        .unwrap();
    assert!(
        engine
            .evaluate(&script)
            .unwrap()
            .value()
            .same_value(&Value::Boolean(true))
    );
}

#[test]
fn json_reviver_rejects_foreign_objects_at_child_and_root_boundaries() {
    let context = temiya::runtime::AllocationContext::new();
    let foreign_context = temiya::runtime::AllocationContext::new();
    let foreign = Object::new_in(&foreign_context);
    for (source, replaced_key) in [("null", ""), (r#"{"child":0}"#, "child")] {
        let mut reviver = |key: &str, value: Value| {
            Ok(if key == replaced_key {
                Value::Object(foreign.clone())
            } else {
                value
            })
        };
        assert_eq!(
            Json::parse(&context, &JsString::from(source), Some(&mut reviver))
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::TypeError
        );
        assert_eq!(context.collect_garbage().unwrap().retained, 0);
    }
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
