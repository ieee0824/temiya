use std::{cell::RefCell, rc::Rc};

use temiya::runtime::{IntrinsicId, Object, PropertyDescriptor, Realm, RuntimeErrorKind, Value};

fn method(holder: &Object, name: &str) -> Object {
    match holder.get(&name.into()).unwrap() {
        Value::Object(method) => method,
        value => panic!("expected method, got {value:?}"),
    }
}

fn array(values: &[Value]) -> Object {
    let object = Object::new();
    for (index, value) in values.iter().cloned().enumerate() {
        object
            .create_data_property(index.to_string().as_str().into(), value)
            .unwrap();
    }
    object
        .create_data_property("length".into(), Value::Number(values.len() as f64))
        .unwrap();
    object
}

#[test]
fn object_methods_convert_and_apply_property_descriptors() {
    let realm = Realm::ordinary();
    let constructor = realm.intrinsic(IntrinsicId::Object);
    let target = Object::new();
    let descriptor = Object::new();
    descriptor
        .create_data_property("value".into(), Value::Number(42.0))
        .unwrap();
    descriptor
        .create_data_property("writable".into(), Value::Boolean(false))
        .unwrap();

    method(&constructor, "defineProperty")
        .call(
            Value::Undefined,
            &[
                Value::Object(target.clone()),
                Value::String("answer".into()),
                Value::Object(descriptor),
            ],
        )
        .unwrap();
    assert!(matches!(
        target.get(&"answer".into()).unwrap(),
        Value::Number(42.0)
    ));
    assert!(!target.set(&"answer".into(), Value::Number(0.0)).unwrap());

    let result = method(&constructor, "getOwnPropertyDescriptor")
        .call(
            Value::Undefined,
            &[Value::Object(target), Value::String("answer".into())],
        )
        .unwrap();
    let Value::Object(result) = result else {
        panic!()
    };
    assert!(matches!(
        result.get(&"writable".into()).unwrap(),
        Value::Boolean(false)
    ));
}

#[test]
fn call_apply_and_bind_preserve_receiver_and_argument_order() {
    let realm = Realm::ordinary();
    let prototype = realm.intrinsic(IntrinsicId::FunctionPrototype);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let calls = observed.clone();
    let function = Object::native_function(move |this, arguments| {
        calls.borrow_mut().push((this, arguments.to_vec()));
        Ok(arguments.last().cloned().unwrap_or(Value::Undefined))
    });

    let result = method(&prototype, "call")
        .call(
            Value::Object(function.clone()),
            &[Value::Number(1.0), Value::Number(2.0)],
        )
        .unwrap();
    assert!(matches!(result, Value::Number(2.0)));
    method(&prototype, "apply")
        .call(
            Value::Object(function.clone()),
            &[
                Value::Number(3.0),
                Value::Object(array(&[Value::Number(4.0)])),
            ],
        )
        .unwrap();
    let bound = method(&prototype, "bind")
        .call(
            Value::Object(function),
            &[Value::Number(5.0), Value::Number(6.0)],
        )
        .unwrap();
    let Value::Object(bound) = bound else {
        panic!()
    };
    bound.call(Value::Null, &[Value::Number(7.0)]).unwrap();

    let calls = observed.borrow();
    assert!(matches!(calls[0].0, Value::Number(1.0)));
    assert_eq!(calls[2].1.len(), 2);
    assert!(matches!(calls[2].1[0], Value::Number(6.0)));
    assert!(matches!(calls[2].1[1], Value::Number(7.0)));
}

#[test]
fn reflect_operations_return_booleans_and_include_symbol_keys() {
    let realm = Realm::ordinary();
    let reflect = realm.intrinsic(IntrinsicId::Reflect);
    let target = Object::new();
    let descriptor = PropertyDescriptor::data(Value::Number(1.0), true, true, true)
        .to_object()
        .unwrap();
    let defined = method(&reflect, "defineProperty")
        .call(
            Value::Undefined,
            &[
                Value::Object(target.clone()),
                Value::String("x".into()),
                Value::Object(descriptor),
            ],
        )
        .unwrap();
    assert!(matches!(defined, Value::Boolean(true)));
    assert!(matches!(
        method(&reflect, "has")
            .call(
                Value::Undefined,
                &[Value::Object(target.clone()), Value::String("x".into())]
            )
            .unwrap(),
        Value::Boolean(true)
    ));
    let keys = method(&reflect, "ownKeys")
        .call(Value::Undefined, &[Value::Object(target.clone())])
        .unwrap();
    let Value::Object(keys) = keys else { panic!() };
    assert!(matches!(keys.get(&"0".into()).unwrap(), Value::String(value) if value == "x".into()));
    assert!(matches!(
        method(&reflect, "deleteProperty")
            .call(
                Value::Undefined,
                &[Value::Object(target), Value::String("x".into())]
            )
            .unwrap(),
        Value::Boolean(true)
    ));
}

#[test]
fn reflection_rejects_invalid_targets_and_argument_lists() {
    let realm = Realm::ordinary();
    let reflect = realm.intrinsic(IntrinsicId::Reflect);
    let error = method(&reflect, "getPrototypeOf")
        .call(Value::Undefined, &[Value::Null])
        .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::TypeError);

    let function = Object::native_function(|_, _| Ok(Value::Undefined));
    let invalid = Object::new();
    invalid
        .create_data_property("length".into(), Value::Number(f64::INFINITY))
        .unwrap();
    let error = method(&reflect, "apply")
        .call(
            Value::Undefined,
            &[
                Value::Object(function),
                Value::Undefined,
                Value::Object(invalid),
            ],
        )
        .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::RangeError);
}

#[test]
fn reflect_construct_and_bound_construction_forward_arguments() {
    let realm = Realm::ordinary();
    let reflect = realm.intrinsic(IntrinsicId::Reflect);
    let function_prototype = realm.intrinsic(IntrinsicId::FunctionPrototype);
    let constructor = Object::native_callable_constructor(
        |_, _| Ok(Value::Undefined),
        |arguments, _| {
            let instance = Object::new();
            instance.create_data_property(
                "value".into(),
                arguments.first().cloned().unwrap_or(Value::Undefined),
            )?;
            Ok(instance)
        },
    );
    let bound = method(&function_prototype, "bind")
        .call(
            Value::Object(constructor),
            &[Value::Null, Value::Number(9.0)],
        )
        .unwrap();
    let result = method(&reflect, "construct")
        .call(Value::Undefined, &[bound, Value::Object(array(&[]))])
        .unwrap();
    let Value::Object(result) = result else {
        panic!()
    };
    assert!(matches!(
        result.get(&"value".into()).unwrap(),
        Value::Number(9.0)
    ));
}
