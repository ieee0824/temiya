use temiya::runtime::{
    IntrinsicId, Object, PropertyKey, Realm, RuntimeErrorKind, Value, WellKnownSymbol,
};

#[test]
fn realm_installs_intrinsics_with_standard_descriptors() {
    let realm = Realm::ordinary();
    let global = realm.global_object();
    let descriptor = global
        .get_own_property(&PropertyKey::from("Object"))
        .unwrap();
    assert_eq!(descriptor.writable, Some(true));
    assert_eq!(descriptor.enumerable, Some(false));
    assert_eq!(descriptor.configurable, Some(true));
    assert!(
        matches!(descriptor.value, Some(Value::Object(value)) if value == realm.intrinsic(IntrinsicId::Object))
    );

    for name in ["undefined", "NaN", "Infinity"] {
        let descriptor = global.get_own_property(&name.into()).unwrap();
        assert_eq!(descriptor.writable, Some(false));
        assert_eq!(descriptor.enumerable, Some(false));
        assert_eq!(descriptor.configurable, Some(false));
    }
}

#[test]
fn initialization_wires_circular_constructor_and_prototype_links() {
    let realm = Realm::ordinary();
    let object = realm.intrinsic(IntrinsicId::Object);
    let object_prototype = realm.intrinsic(IntrinsicId::ObjectPrototype);
    let function_prototype = realm.intrinsic(IntrinsicId::FunctionPrototype);

    assert_eq!(object.prototype(), Some(function_prototype.clone()));
    assert_eq!(
        function_prototype.prototype(),
        Some(object_prototype.clone())
    );
    assert_eq!(object_prototype.prototype(), None);
    assert!(
        matches!(object.get(&"prototype".into()).unwrap(), Value::Object(value) if value == object_prototype)
    );
    assert!(
        matches!(object_prototype.get(&"constructor".into()).unwrap(), Value::Object(value) if value == object)
    );
}

#[test]
fn registry_is_complete_deterministic_and_realm_local() {
    let first = Realm::ordinary();
    let second = Realm::ordinary();
    let ids = first
        .intrinsics()
        .iter()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(ids, IntrinsicId::ALL);
    for id in IntrinsicId::ALL {
        assert_ne!(first.intrinsic(id), second.intrinsic(id));
    }
    assert_ne!(
        first.well_known_symbol(WellKnownSymbol::Iterator),
        second.well_known_symbol(WellKnownSymbol::Iterator)
    );
}

#[test]
fn host_global_that_rejects_properties_returns_an_error() {
    let global = Object::new();
    global.prevent_extensions();
    let error = Realm::try_new(global.clone(), Value::Object(global)).unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::TypeError);
    assert!(
        error
            .message()
            .contains("cannot install intrinsic property")
    );
}
