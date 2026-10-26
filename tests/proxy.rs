use std::{cell::RefCell, rc::Rc};

use temiya::runtime::{
    IntrinsicId, Object, PropertyDescriptor, ProxyObject, Realm, RuntimeErrorKind, Value,
};

fn method(object: &Object, name: &str) -> Object {
    let Value::Object(function) = object.get(&name.into()).unwrap() else {
        panic!("{name} is not callable")
    };
    function
}

#[test]
fn proxy_constructor_and_revocable_are_installed() {
    let realm = Realm::ordinary();
    let constructor = realm.intrinsic(IntrinsicId::Proxy);
    assert_eq!(
        constructor.call(Value::Undefined, &[]).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );

    let result = method(&constructor, "revocable")
        .call(
            Value::Undefined,
            &[Value::Object(Object::new()), Value::Object(Object::new())],
        )
        .unwrap();
    let Value::Object(result) = result else {
        panic!()
    };
    let Value::Object(proxy) = result.get(&"proxy".into()).unwrap() else {
        panic!()
    };
    let Value::Object(revoke) = result.get(&"revoke".into()).unwrap() else {
        panic!()
    };
    revoke.call(Value::Undefined, &[]).unwrap();

    let reflect = realm.intrinsic(IntrinsicId::Reflect);
    assert_eq!(
        method(&reflect, "get")
            .call(
                Value::Undefined,
                &[Value::Object(proxy), Value::String("x".into())],
            )
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn get_set_has_and_delete_traps_receive_target_and_keys() {
    let target = Object::new();
    target
        .create_data_property("x".into(), Value::Number(1.0))
        .unwrap();
    let handler = Object::new();
    handler
        .create_data_property(
            "get".into(),
            Value::Object(Object::native_function(|_, arguments| {
                let Value::String(key) = &arguments[1] else {
                    panic!()
                };
                Ok(Value::String(
                    format!("trap:{}", key.to_string_lossy()).into(),
                ))
            })),
        )
        .unwrap();
    handler
        .create_data_property(
            "has".into(),
            Value::Object(Object::native_function(|_, _| Ok(Value::Boolean(false)))),
        )
        .unwrap();
    let proxy = ProxyObject::new(target, handler).object();
    assert!(
        matches!(proxy.get(&"x".into()).unwrap(), Value::String(value) if value == "trap:x".into())
    );
    assert!(!proxy.has_property(&"x".into()).unwrap());
}

#[test]
fn target_invariants_reject_lying_traps() {
    let target = Object::new();
    target
        .define_own_property(
            "locked".into(),
            PropertyDescriptor::data(Value::Number(1.0), false, true, false),
        )
        .unwrap();
    let handler = Object::new();
    handler
        .create_data_property(
            "get".into(),
            Value::Object(Object::native_function(|_, _| Ok(Value::Number(2.0)))),
        )
        .unwrap();
    handler
        .create_data_property(
            "deleteProperty".into(),
            Value::Object(Object::native_function(|_, _| Ok(Value::Boolean(true)))),
        )
        .unwrap();
    let proxy = ProxyObject::new(target, handler).object();
    assert_eq!(
        proxy.get(&"locked".into()).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(
        proxy.try_delete(&"locked".into()).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn own_keys_enforces_duplicates_and_non_configurable_keys() {
    let target = Object::new();
    target
        .define_own_property(
            "fixed".into(),
            PropertyDescriptor::data(Value::Undefined, true, true, false),
        )
        .unwrap();
    let result = Object::new();
    result
        .create_data_property("0".into(), Value::String("other".into()))
        .unwrap();
    result
        .create_data_property("length".into(), Value::Number(1.0))
        .unwrap();
    let handler = Object::new();
    handler
        .create_data_property(
            "ownKeys".into(),
            Value::Object(Object::native_function(move |_, _| {
                Ok(Value::Object(result.clone()))
            })),
        )
        .unwrap();
    let proxy = ProxyObject::new(target, handler).object();
    assert_eq!(
        proxy.try_own_property_keys().unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn revocation_invalidates_objects_calls_and_construction() {
    let target = Object::native_callable_constructor(
        |_, _| Ok(Value::Number(1.0)),
        |_, _| Ok(Object::new()),
    );
    let proxy = ProxyObject::new(target, Object::new());
    assert!(matches!(
        proxy.object().call(Value::Undefined, &[]).unwrap(),
        Value::Number(1.0)
    ));
    proxy.revoke();
    assert_eq!(
        proxy.object().get(&"x".into()).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(
        proxy
            .object()
            .call(Value::Undefined, &[])
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(
        proxy
            .object()
            .construct(&[], proxy.object())
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn traps_can_reenter_the_same_proxy() {
    let target = Object::new();
    target
        .create_data_property("inner".into(), Value::Number(9.0))
        .unwrap();
    let holder = Rc::new(RefCell::new(None::<Object>));
    let captured = holder.clone();
    let handler = Object::new();
    handler
        .create_data_property(
            "get".into(),
            Value::Object(Object::native_function(move |_, arguments| {
                let Value::String(key) = &arguments[1] else {
                    panic!()
                };
                if key == &"outer".into() {
                    captured.borrow().as_ref().unwrap().get(&"inner".into())
                } else {
                    let Value::Object(target) = &arguments[0] else {
                        panic!()
                    };
                    target.get(&key.clone().into())
                }
            })),
        )
        .unwrap();
    let proxy = ProxyObject::new(target, handler).object();
    *holder.borrow_mut() = Some(proxy.clone());
    assert!(matches!(
        proxy.get(&"outer".into()).unwrap(),
        Value::Number(9.0)
    ));
}
