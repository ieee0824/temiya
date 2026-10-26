use std::{cell::RefCell, rc::Rc};

use temiya::{
    parser::{ParserOptions, parse_expression_with_options},
    runtime::{
        ExpressionEvaluator, IntrinsicId, JsString, Realm, RegExp, RuntimeErrorKind, StringObject,
        Value,
    },
};

#[test]
fn string_exotic_exposes_utf16_indices_and_fixed_length() {
    let string = StringObject::new(JsString::from("😀a"));
    let object = string.object();
    assert!(matches!(
        object.get(&"length".into()).unwrap(),
        Value::Number(3.0)
    ));
    assert!(object.get_own_property(&"0".into()).is_some());
    assert!(object.get_own_property(&"1".into()).is_some());
    assert!(!object.delete(&"0".into()));
    assert!(!object.set(&"length".into(), Value::Number(0.0)).unwrap());
}

#[test]
fn unicode_global_match_tracks_utf16_last_index() {
    let regexp = RegExp::new(JsString::from("."), "gu").unwrap();
    let input = JsString::from("😀a");
    let first = regexp.exec(&input).unwrap().unwrap();
    assert_eq!(first.index, 0);
    assert_eq!(first.captures[0], Some(JsString::from("😀")));
    assert_eq!(regexp.last_index(), 2);
    let second = regexp.exec(&input).unwrap().unwrap();
    assert_eq!(second.index, 2);
    assert_eq!(regexp.last_index(), 3);
    assert!(regexp.exec(&input).unwrap().is_none());
    assert_eq!(regexp.last_index(), 0);
}

#[test]
fn named_captures_and_replace_callback_are_observable() {
    let regexp = RegExp::new(JsString::from("(?<word>[a-z]+)"), "g").unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let observed = seen.clone();
    let output = regexp
        .replace(&JsString::from("one two"), move |found| {
            observed.borrow_mut().push(found.named_captures.clone());
            Ok(JsString::from("X"))
        })
        .unwrap();
    assert_eq!(output, JsString::from("X X"));
    assert_eq!(seen.borrow().len(), 2);
    assert_eq!(
        seen.borrow()[0][0],
        ("word".to_owned(), Some(JsString::from("one")))
    );
}

#[test]
fn sticky_and_invalid_flags_cover_boundaries() {
    let sticky = RegExp::new(JsString::from("a"), "y").unwrap();
    sticky.set_last_index(1);
    assert!(sticky.exec(&JsString::from("ba")).unwrap().is_some());
    sticky.set_last_index(0);
    assert!(sticky.exec(&JsString::from("ba")).unwrap().is_none());
    assert_eq!(
        RegExp::new(JsString::from("a"), "gg").unwrap_err().kind(),
        RuntimeErrorKind::SyntaxError
    );
}

#[test]
fn realm_constructors_and_literals_create_runtime_regexp_objects() {
    let realm = Realm::ordinary();
    let string = realm.intrinsic(IntrinsicId::String);
    assert!(matches!(
        string.call(Value::Undefined, &[Value::Number(12.0)]).unwrap(),
        Value::String(value) if value == JsString::from("12")
    ));

    let evaluator = ExpressionEvaluator::new(realm);
    let expression =
        parse_expression_with_options("/(?<letter>a)/g", ParserOptions::default()).unwrap();
    let Value::Object(object) = evaluator.evaluate(&expression).unwrap() else {
        panic!()
    };
    let regexp = RegExp::from_object(object).unwrap();
    assert!(regexp.exec(&JsString::from("a")).unwrap().is_some());
}
