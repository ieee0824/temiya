use std::cell::RefCell;
use std::rc::Rc;

use temiya::runtime::{
    JsString, Object, PropertyDescriptor, PropertyKey, RuntimeErrorKind, Symbol, Value,
    is_compatible_property_descriptor,
};

fn key(value: &str) -> PropertyKey {
    PropertyKey::from(value)
}

fn assert_same(actual: Value, expected: Value) {
    assert!(actual.same_value(&expected), "{actual:?} != {expected:?}");
}

#[test]
fn property_keys_distinguish_strings_symbols_and_array_indices() {
    assert_eq!(key("0").array_index(), Some(0));
    assert_eq!(key("4294967294").array_index(), Some(4_294_967_294));
    for non_index in ["", "00", "01", "-0", "1.0", "4294967295"] {
        assert_eq!(key(non_index).array_index(), None, "{non_index}");
    }

    let description = JsString::from("same");
    assert_ne!(
        PropertyKey::from(Symbol::new(Some(description.clone()))),
        PropertyKey::from(Symbol::new(Some(description)))
    );

    let symbol = Symbol::new(None);
    assert_eq!(
        Value::Symbol(symbol.clone()).to_property_key().unwrap(),
        PropertyKey::Symbol(symbol)
    );
    assert_eq!(Value::Number(-0.0).to_property_key().unwrap(), key("0"));
}

#[test]
fn descriptor_conversion_round_trips_present_fields() {
    let context = temiya::runtime::AllocationContext::new();
    let getter = Object::native_function_in(&context, |_, _| Ok(Value::Number(3.0)));
    let descriptor = PropertyDescriptor {
        get: Some(Some(getter.clone())),
        set: Some(None),
        enumerable: Some(true),
        configurable: Some(false),
        ..PropertyDescriptor::default()
    };
    let object = descriptor.to_object(&context).unwrap();
    let converted = PropertyDescriptor::from_object(&object).unwrap();
    assert_eq!(converted.get.flatten(), Some(getter));
    assert_eq!(converted.set, Some(None));
    assert_eq!(converted.enumerable, Some(true));
    assert_eq!(converted.configurable, Some(false));
    assert!(converted.value.is_none());
    assert!(converted.writable.is_none());

    let invalid = Object::new_in(&context);
    invalid
        .create_data_property("get".into(), Value::Number(1.0))
        .unwrap();
    assert_eq!(
        PropertyDescriptor::from_object(&invalid)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn descriptor_conversion_observes_inherited_fields_and_getters() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let prototype = Object::new_in(&allocation_context);
    prototype
        .create_data_property("enumerable".into(), Value::Boolean(true))
        .unwrap();
    let calls = Rc::new(RefCell::new(0));
    let observed = Rc::clone(&calls);
    let getter = Object::native_function_in(&allocation_context, move |this, _| {
        *observed.borrow_mut() += 1;
        assert!(matches!(this, Value::Object(_)));
        Ok(Value::Boolean(true))
    });
    prototype
        .define_own_property(
            "configurable".into(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        )
        .unwrap();
    let object = Object::new_in(&allocation_context);
    assert!(object.set_prototype(Some(prototype)));

    let descriptor = PropertyDescriptor::from_object(&object).unwrap();
    assert_eq!(descriptor.enumerable, Some(true));
    assert_eq!(descriptor.configurable, Some(true));
    assert_eq!(*calls.borrow(), 1);
}

#[test]
fn descriptor_invariants_and_partial_attribute_updates_are_enforced() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let object = Object::new_in(&allocation_context);
    assert!(
        object
            .define_own_property(
                key("fixed"),
                PropertyDescriptor::data(Value::Number(f64::NAN), false, true, false),
            )
            .unwrap()
    );
    assert!(
        object
            .define_own_property(
                key("fixed"),
                PropertyDescriptor {
                    value: Some(Value::Number(f64::NAN)),
                    ..PropertyDescriptor::default()
                },
            )
            .unwrap()
    );
    for incompatible in [
        PropertyDescriptor {
            value: Some(Value::Number(1.0)),
            ..PropertyDescriptor::default()
        },
        PropertyDescriptor {
            writable: Some(true),
            ..PropertyDescriptor::default()
        },
        PropertyDescriptor {
            enumerable: Some(false),
            ..PropertyDescriptor::default()
        },
        PropertyDescriptor {
            configurable: Some(true),
            ..PropertyDescriptor::default()
        },
    ] {
        let current = object.get_own_property(&key("fixed")).unwrap();
        assert!(!is_compatible_property_descriptor(true, &incompatible, Some(&current)).unwrap());
        assert!(
            !object
                .define_own_property(key("fixed"), incompatible)
                .unwrap()
        );
    }

    assert!(
        object
            .define_own_property(
                key("mutable"),
                PropertyDescriptor::data(Value::Number(1.0), true, false, true),
            )
            .unwrap()
    );
    assert!(
        object
            .define_own_property(
                key("mutable"),
                PropertyDescriptor {
                    value: Some(Value::Number(2.0)),
                    writable: Some(false),
                    ..PropertyDescriptor::default()
                },
            )
            .unwrap()
    );
    let updated = object.get_own_property(&key("mutable")).unwrap();
    assert!(updated.value.unwrap().same_value(&Value::Number(2.0)));
    assert_eq!(updated.writable, Some(false));
    assert_eq!(updated.enumerable, Some(false));
    assert_eq!(updated.configurable, Some(true));

    object
        .define_own_property(
            key("zero"),
            PropertyDescriptor::data(Value::Number(-0.0), false, false, false),
        )
        .unwrap();
    assert!(
        !object
            .define_own_property(
                key("zero"),
                PropertyDescriptor {
                    value: Some(Value::Number(0.0)),
                    ..PropertyDescriptor::default()
                },
            )
            .unwrap()
    );
}

#[test]
fn descriptor_kind_changes_require_configurability() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let object = Object::new_in(&allocation_context);
    object
        .define_own_property(
            key("open"),
            PropertyDescriptor::data(Value::Number(1.0), true, true, true),
        )
        .unwrap();
    let getter = Object::native_function_in(&allocation_context, |_, _| Ok(Value::Number(2.0)));
    assert!(
        object
            .define_own_property(
                key("open"),
                PropertyDescriptor {
                    get: Some(Some(getter.clone())),
                    ..PropertyDescriptor::default()
                },
            )
            .unwrap()
    );
    let changed = object.get_own_property(&key("open")).unwrap();
    assert_eq!(changed.get.flatten(), Some(getter));
    assert!(changed.value.is_none());

    let mixed = PropertyDescriptor {
        value: Some(Value::Undefined),
        get: Some(None),
        ..PropertyDescriptor::default()
    };
    assert_eq!(
        object
            .define_own_property(key("invalid"), mixed)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn ordinary_get_and_set_preserve_the_receiver_across_prototypes() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let prototype = Object::new_in(&allocation_context);
    let getter = Object::native_function_in(&allocation_context, |this, _| {
        let Value::Object(receiver) = this else {
            panic!("receiver was not an object");
        };
        receiver.get(&key("marker"))
    });
    let assigned = Rc::new(RefCell::new(None));
    let assigned_by_setter = Rc::clone(&assigned);
    let setter = Object::native_function_in(&allocation_context, move |this, arguments| {
        let Value::Object(receiver) = this else {
            panic!("receiver was not an object");
        };
        *assigned_by_setter.borrow_mut() = Some((receiver.identity(), arguments[0].clone()));
        Ok(Value::Undefined)
    });
    prototype
        .define_own_property(
            key("accessor"),
            PropertyDescriptor::accessor(Some(getter), Some(setter), true, true),
        )
        .unwrap();
    prototype
        .define_own_property(
            key("inherited"),
            PropertyDescriptor::data(Value::Number(1.0), true, true, true),
        )
        .unwrap();

    let object = Object::new_in(&allocation_context);
    object
        .create_data_property(key("marker"), Value::String(JsString::from("child")))
        .unwrap();
    assert!(object.set_prototype(Some(prototype.clone())));
    assert_same(
        object.get(&key("accessor")).unwrap(),
        Value::String(JsString::from("child")),
    );
    assert!(object.set(&key("accessor"), Value::Number(9.0)).unwrap());
    let (identity, value) = assigned.borrow_mut().take().unwrap();
    assert_eq!(identity, object.identity());
    assert_same(value, Value::Number(9.0));

    assert!(object.set(&key("inherited"), Value::Number(2.0)).unwrap());
    assert_same(object.get(&key("inherited")).unwrap(), Value::Number(2.0));
    assert_same(
        prototype.get(&key("inherited")).unwrap(),
        Value::Number(1.0),
    );
}

#[test]
fn ordinary_objects_reject_forbidden_writes_and_deletes() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let object = Object::new_in(&allocation_context);
    object
        .define_own_property(
            key("locked"),
            PropertyDescriptor::data(Value::Number(1.0), false, false, false),
        )
        .unwrap();
    assert!(!object.set(&key("locked"), Value::Number(2.0)).unwrap());
    assert!(!object.delete(&key("locked")));
    assert!(object.delete(&key("missing")));

    object.prevent_extensions();
    assert!(!object.is_extensible());
    assert!(
        !object
            .create_data_property(key("new"), Value::Undefined)
            .unwrap()
    );
    assert!(!object.set(&key("new"), Value::Undefined).unwrap());
}

#[test]
fn own_property_keys_follow_ecmascript_order() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let object = Object::new_in(&allocation_context);
    let first_symbol = Symbol::new(Some(JsString::from("first")));
    let second_symbol = Symbol::new(Some(JsString::from("second")));
    for property in [
        key("beta"),
        key("10"),
        PropertyKey::from(first_symbol.clone()),
        key("2"),
        key("01"),
        PropertyKey::from(second_symbol.clone()),
    ] {
        object
            .create_data_property(property, Value::Undefined)
            .unwrap();
    }
    assert_eq!(
        object.own_property_keys(),
        vec![
            key("2"),
            key("10"),
            key("beta"),
            key("01"),
            PropertyKey::from(first_symbol),
            PropertyKey::from(second_symbol),
        ]
    );
}

#[test]
fn prototype_changes_reject_cycles_and_non_extensible_objects() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let first = Object::new_in(&allocation_context);
    let second = Object::new_in(&allocation_context);
    let third = Object::new_in(&allocation_context);
    assert!(first.set_prototype(Some(second.clone())));
    assert!(second.set_prototype(Some(third.clone())));
    assert!(!third.set_prototype(Some(first)));

    second.prevent_extensions();
    assert!(second.set_prototype(Some(third.clone())));
    assert!(!second.set_prototype(None));
    assert_eq!(second.prototype(), Some(third));
}

#[test]
fn create_data_property_replaces_configurable_accessors_without_calling_them() {
    let context = temiya::runtime::AllocationContext::new();
    let object = Object::new_in(&context);
    let getter = Object::native_function_in(&context, |_, _| {
        panic!("replacing a descriptor must not invoke its getter")
    });
    object
        .define_own_property(
            key("x"),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        )
        .unwrap();
    object
        .create_data_property(key("last"), Value::Undefined)
        .unwrap();
    object.prevent_extensions();
    let child = Object::new_in(&context);
    child
        .create_data_property(key("self"), Value::Object(child.clone()))
        .unwrap();
    assert!(
        object
            .create_data_property(key("x"), Value::Object(child.clone()))
            .unwrap()
    );
    drop(child);
    context.collect_garbage().unwrap();
    let descriptor = object.get_own_property(&key("x")).unwrap();
    assert!(descriptor.is_data_descriptor());
    assert_eq!(descriptor.writable, Some(true));
    assert_eq!(descriptor.enumerable, Some(true));
    assert_eq!(descriptor.configurable, Some(true));
    let Some(Value::Object(child)) = descriptor.value else {
        panic!("object value")
    };
    assert!(
        child
            .get(&key("self"))
            .unwrap()
            .same_value(&Value::Object(child.clone()))
    );
    assert_eq!(object.own_property_keys(), vec![key("x"), key("last")]);
    assert!(
        !object
            .create_data_property(key("new"), Value::Undefined)
            .unwrap()
    );
}

#[test]
fn create_data_property_rejects_foreign_edges_atomically_even_when_nonextensible() {
    let context = temiya::runtime::AllocationContext::new();
    let foreign_context = temiya::runtime::AllocationContext::new();
    let object = Object::new_in(&context);
    object
        .create_data_property(key("x"), Value::Number(42.0))
        .unwrap();
    object
        .define_own_property(
            key("locked"),
            PropertyDescriptor::data(Value::Number(1.0), true, true, false),
        )
        .unwrap();
    object.prevent_extensions();
    let foreign = Object::new_in(&foreign_context);
    for name in ["x", "locked", "new"] {
        assert!(
            object
                .create_data_property(key(name), Value::Object(foreign.clone()))
                .is_err()
        );
    }
    assert_same(object.get(&key("x")).unwrap(), Value::Number(42.0));
    assert_same(object.get(&key("locked")).unwrap(), Value::Number(1.0));
    assert!(
        !object
            .create_data_property(key("locked"), Value::Number(2.0))
            .unwrap()
    );
    assert_eq!(object.own_property_keys(), vec![key("x"), key("locked")]);
    context.collect_garbage().unwrap();
}
