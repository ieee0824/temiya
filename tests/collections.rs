use temiya::runtime::{
    IntrinsicId, MapObject, Object, Realm, RuntimeErrorKind, SetObject, Value, WeakMapObject,
    WeakSetObject,
};

#[test]
fn map_and_set_use_same_value_zero_and_insertion_order() {
    let map = MapObject::new();
    map.set(Value::Number(f64::NAN), Value::Number(1.0));
    map.set(Value::Number(f64::NAN), Value::Number(2.0));
    map.set(Value::Number(-0.0), Value::String("zero".into()));
    assert_eq!(map.size(), 2);
    assert!(matches!(
        map.get(&Value::Number(f64::NAN)),
        Value::Number(2.0)
    ));
    assert!(map.has(&Value::Number(0.0)));
    let entries = map.entries().collect::<Vec<_>>();
    assert!(matches!(entries[0].0, Value::Number(value) if value.is_nan()));

    let set = SetObject::new();
    set.add(Value::Number(-0.0));
    set.add(Value::Number(0.0));
    set.add(Value::Number(f64::NAN));
    set.add(Value::Number(f64::NAN));
    assert_eq!(set.size(), 2);
}

#[test]
fn live_iterators_observe_deletion_and_later_addition() {
    let map = MapObject::new();
    map.set(Value::String("a".into()), Value::Number(1.0));
    map.set(Value::String("b".into()), Value::Number(2.0));
    let mut iterator = map.entries();
    assert!(matches!(
        iterator.next(),
        Some((Value::String(_), Value::Number(1.0)))
    ));
    assert!(map.delete(&Value::String("b".into())));
    map.set(Value::String("c".into()), Value::Number(3.0));
    assert!(matches!(
        iterator.next(),
        Some((Value::String(_), Value::Number(3.0)))
    ));
    assert!(iterator.next().is_none());

    let set = SetObject::new();
    set.add(Value::Number(1.0));
    let mut values = set.values();
    assert!(matches!(values.next(), Some(Value::Number(1.0))));
    set.add(Value::Number(2.0));
    assert!(matches!(values.next(), Some(Value::Number(2.0))));
}

#[test]
fn weak_collections_do_not_retain_keys() {
    let map = WeakMapObject::new();
    let set = WeakSetObject::new();
    let key = Object::new();
    map.set(&Value::Object(key.clone()), Value::String("value".into()))
        .unwrap();
    set.add(&Value::Object(key.clone())).unwrap();
    assert_eq!(map.live_len(), 1);
    assert_eq!(set.live_len(), 1);
    drop(key);
    assert_eq!(map.live_len(), 0);
    assert_eq!(set.live_len(), 0);
}

#[test]
fn weak_collections_reject_primitive_keys() {
    let map = WeakMapObject::new();
    let set = WeakSetObject::new();
    assert_eq!(
        map.set(&Value::Number(1.0), Value::Undefined)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(
        set.add(&Value::String("x".into())).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn realm_collection_constructors_and_methods_use_internal_slots() {
    let realm = Realm::ordinary();
    let constructor = realm.intrinsic(IntrinsicId::Map);
    let object = constructor.construct(&[], constructor.clone()).unwrap();
    let prototype = realm.intrinsic(IntrinsicId::MapPrototype);
    let Value::Object(set) = prototype.get(&"set".into()).unwrap() else {
        panic!()
    };
    set.call(
        Value::Object(object.clone()),
        &[Value::String("key".into()), Value::Number(7.0)],
    )
    .unwrap();
    assert!(matches!(
        MapObject::from_object(object)
            .unwrap()
            .get(&Value::String("key".into())),
        Value::Number(7.0)
    ));
    assert!(!constructor.is_callable());
}
