use std::{collections::BTreeMap, rc::Rc};

use temiya::{
    Engine,
    runtime::{ArrayObject, Object, PropertyDescriptor, PropertyKey, Symbol, Value},
};

fn check(source: &str) {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine.compile(&realm, source).unwrap();
    let result = engine.evaluate(&script).unwrap();
    assert!(
        matches!(result.value(), Value::Boolean(true)),
        "{source}\n{result:?}"
    );
}

#[test]
fn dense_holey_sparse_roundtrip_preserves_values_and_absence() {
    check(
        "var a = []; for (var i = 0; i < 128; i++) a.push(i); \
         delete a[17]; a[4294967294] = 900; \
         var ok = a.length === 4294967295 && !(17 in a) && a[127] === 127; \
         a.length = 128; a[17] = undefined; \
         ok = ok && a.length === 128 && (17 in a) && a[17] === undefined; \
         for (var i = 0; i < 120; i++) delete a[i]; \
         ok = ok && !(0 in a) && a[120] === 120; \
         for (var i = 0; i < 120; i++) a[i] = i; \
         a.length = 0; a.push(42); ok && a.length === 1 && a[0] === 42 && !(127 in a);",
    );
}

#[test]
fn sparse_length_shrink_preserves_nonindex_keys_and_enumeration_order() {
    let array = ArrayObject::new(u32::MAX).unwrap().object();
    let symbol = Symbol::new(None);
    for name in ["4294967295", "01", "-0", "named", "4294967294", "7", "0"] {
        assert!(
            array
                .create_data_property(name.into(), Value::Number(1.0))
                .unwrap()
        );
    }
    assert!(
        array
            .create_data_property(symbol.clone().into(), Value::Number(2.0))
            .unwrap()
    );
    let keys: Vec<PropertyKey> = [
        "0",
        "7",
        "4294967294",
        "length",
        "4294967295",
        "01",
        "-0",
        "named",
    ]
    .into_iter()
    .map(Into::into)
    .chain([symbol.clone().into()])
    .collect();
    assert_eq!(array.own_property_keys(), keys);
    assert!(array.set(&"length".into(), Value::Number(0.0)).unwrap());
    assert_eq!(array.own_property_keys(), keys[3..]);
    assert!(
        array
            .get(&symbol.into())
            .unwrap()
            .same_value(&Value::Number(2.0))
    );
}

#[test]
fn prototype_getter_and_setter_can_mutate_the_element_representation() {
    check(
        "var a = [0, 1, 2]; delete a[1]; var proto = {}; \
         Object.defineProperty(proto, '1', {get() { \
           this[4294967294] = 8; this.length = 3; return 41; \
         }, set(v) { this[4294967294] = v; }}); \
         Object.setPrototypeOf(a, proto); \
         var ok = a[1] === 41 && a.length === 3 && !(4294967294 in a); \
         a[1] = 77; ok && a[4294967294] === 77 && a.length === 4294967295;",
    );
    check(
        "var a = [0, 1, 2]; Object.defineProperty(a, '1', { \
          get() { a[4294967294] = 3; a.length = 2; return 9; }, configurable: true }); \
         a[1] === 9 && a.length === 2 && !(2 in a);",
    );
}

#[test]
fn descriptors_and_failed_length_shrink_survive_every_storage_kind() {
    for far_index in ["3", "63", "4294967294"] {
        check(&format!(
            "var a = [0, 1, 2]; a[{far_index}] = 3; \
             Object.defineProperty(a, '1', {{value: 11, writable: false, configurable: false}}); \
             var caught = false; try {{ Object.defineProperty(a, 'length', {{value: 0, writable: false}}); }} \
             catch(e) {{ caught = e.constructor === TypeError; }} \
             var d = Object.getOwnPropertyDescriptor(a, '1'); \
             caught && a.length === 2 && a[0] === 0 && a[1] === 11 && !(2 in a) && \
             !({far_index} in a) && d.writable === false && d.configurable === false && \
             Object.getOwnPropertyDescriptor(a, 'length').writable === false;"
        ));
    }
    check(
        "var a = [0, 1, 2]; var failed = !Reflect.defineProperty(a, 'length', {value: 0, enumerable: true}); \
         failed && a.length === 3 && a[2] === 2;",
    );
    check(
        "var a = []; a[4294967294] = 7; Object.preventExtensions(a); \
         var ok = !Reflect.defineProperty(a, '0', {value: 1}); \
         a[4294967294] = 8; a.length = 0; \
         ok && a.length === 0 && !Reflect.defineProperty(a, '0', {value: 1});",
    );
    check("var a = []; a.length = -0; 1 / a.length === Infinity;");
}

#[test]
fn proxy_traps_still_control_index_access_and_deletion() {
    check(
        "var a = [1, 2]; a[4294967294] = 8; var log = ''; \
         var p = new Proxy(a, {get(t, k, r) { log += 'g' + k + ';'; return Reflect.get(t, k, r); }, \
         deleteProperty(t, k) { log += 'd' + k + ';'; return Reflect.deleteProperty(t, k); }}); \
         var value = p[1]; var removed = delete p[4294967294]; \
         value === 2 && removed && !(4294967294 in a) && log === 'g1;d4294967294;';",
    );
}

#[test]
fn removed_elements_release_owned_values() {
    for index in ["0", "40", "4294967294"] {
        let marker = Rc::new(());
        let captured = Rc::clone(&marker);
        let object = Object::native_function(move |_, _| {
            let _ = &captured;
            Ok(Value::Undefined)
        });
        let array = ArrayObject::new(0).unwrap().object();
        assert!(
            array
                .create_data_property(index.into(), Value::Object(object))
                .unwrap()
        );
        assert_eq!(Rc::strong_count(&marker), 2);
        assert!(array.set(&"length".into(), Value::Number(0.0)).unwrap());
        assert_eq!(Rc::strong_count(&marker), 1);
    }
}

#[test]
fn mixed_index_operations_match_an_independent_ordered_model() {
    let array = ArrayObject::new(0).unwrap();
    let object = array.object();
    let mut model = BTreeMap::<u32, f64>::new();
    let mut length = 0;
    let mut seed = 0x1234_5678_u32;
    for step in 0..4000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let index = if step % 31 == 0 {
            u32::MAX - 1
        } else {
            seed % 128
        };
        let key = PropertyKey::from(index.to_string().as_str());
        match (seed >> 16) % 4 {
            0 | 1 => {
                let value = f64::from(step);
                assert!(
                    object
                        .create_data_property(key, Value::Number(value))
                        .unwrap()
                );
                model.insert(index, value);
                length = length.max(index + 1);
            }
            2 => {
                assert!(object.try_delete(&key).unwrap());
                model.remove(&index);
            }
            _ => {
                length = index;
                assert!(
                    object
                        .define_own_property(
                            "length".into(),
                            PropertyDescriptor {
                                value: Some(Value::Number(f64::from(length))),
                                ..PropertyDescriptor::default()
                            }
                        )
                        .unwrap()
                );
                model.retain(|&key, _| key < length);
            }
        }
        assert_eq!(array.length(), length);
        if step % 37 == 0 {
            let expected: Vec<_> = model
                .keys()
                .map(|index| PropertyKey::from(index.to_string().as_str()))
                .chain([PropertyKey::from("length")])
                .collect();
            assert_eq!(object.own_property_keys(), expected);
            for (&index, &value) in &model {
                assert!(
                    object
                        .get(&PropertyKey::from(index.to_string().as_str()))
                        .unwrap()
                        .same_value(&Value::Number(value))
                );
            }
        }
    }
}
