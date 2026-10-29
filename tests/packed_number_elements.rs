use std::{collections::BTreeMap, rc::Rc};

use temiya::runtime::{
    AllocationContext, ArrayObject, Object, PropertyDescriptor, PropertyKey, Value,
};

#[test]
fn numeric_bits_and_object_edges_survive_storage_transitions_and_collection() {
    let context = AllocationContext::new();
    let array = ArrayObject::new(&context, 0).unwrap();
    let object = array.object();
    let bits = [
        0,
        // Delete this ordinary value so every special payload survives the
        // hole/sparse/descriptor round trip without being overwritten.
        42.0_f64.to_bits(),
        1 << 63,
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        0x7ff8_0000_0000_0042,
        1,
        f64::MAX.to_bits(),
    ];
    for (index, bits) in bits.into_iter().enumerate() {
        assert!(
            object
                .create_data_property(
                    index.to_string().as_str().into(),
                    Value::Number(f64::from_bits(bits))
                )
                .unwrap()
        );
    }
    context.collect_garbage().unwrap();
    let check_bits = |missing: Option<usize>| {
        for (index, bits) in bits.into_iter().enumerate() {
            if missing == Some(index) {
                continue;
            }
            let descriptor = object
                .get_own_property(&index.to_string().as_str().into())
                .unwrap();
            assert_eq!(descriptor.writable, Some(true));
            assert_eq!(descriptor.enumerable, Some(true));
            assert_eq!(descriptor.configurable, Some(true));
            let Some(Value::Number(value)) = descriptor.value else {
                panic!("number required")
            };
            assert_eq!(value.to_bits(), bits, "numeric payload at index {index}");
        }
    };
    check_bits(None);
    assert!(object.try_delete(&"1".into()).unwrap());
    assert!(object.get_own_property(&"1".into()).is_none());
    context.collect_garbage().unwrap();
    check_bits(Some(1));
    assert!(
        object
            .create_data_property("4294967294".into(), Value::Number(42.0))
            .unwrap()
    );
    context.collect_garbage().unwrap();
    check_bits(Some(1));
    assert!(object.set(&"length".into(), Value::Number(8.0)).unwrap());
    assert!(
        object
            .create_data_property("1".into(), Value::Number(42.0))
            .unwrap()
    );
    context.collect_garbage().unwrap();
    check_bits(None);

    // Start the managed-child overwrite from a fresh numeric-only refill.
    assert!(object.set(&"length".into(), Value::Number(0.0)).unwrap());
    assert!(
        object
            .create_data_property("0".into(), Value::Number(0.0))
            .unwrap()
    );

    let marker = Rc::new(());
    let captured = marker.clone();
    let child = Object::native_function_in(&context, move |_, _| {
        let _ = &captured;
        Ok(Value::Number(42.0))
    });
    assert!(
        object
            .create_data_property("0".into(), Value::Object(child))
            .unwrap()
    );
    context.collect_garbage().unwrap();
    assert_eq!(Rc::strong_count(&marker), 2);
    let Value::Object(child) = object.get(&"0".into()).unwrap() else {
        panic!("child required")
    };
    assert!(
        child
            .call(Value::Undefined, &[])
            .unwrap()
            .same_value(&Value::Number(42.0))
    );
    drop(child);
    assert!(
        object
            .create_data_property("0".into(), Value::Number(0.0))
            .unwrap()
    );
    context.collect_garbage().unwrap();
    assert_eq!(Rc::strong_count(&marker), 1);
    drop((array, object));
    assert_eq!(context.collect_garbage().unwrap().retained, 0);
}

#[derive(Clone, Debug)]
enum ModelValue {
    Number(f64),
    Text(&'static str),
}
impl ModelValue {
    fn value(&self) -> Value {
        match self {
            Self::Number(value) => Value::Number(*value),
            Self::Text(value) => Value::String((*value).into()),
        }
    }
    fn matches(&self, value: &Value) -> bool {
        match (self, value) {
            (Self::Number(a), Value::Number(b)) => {
                (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
            }
            (Self::Text(a), Value::String(b)) => {
                a.encode_utf16().eq(b.code_units().iter().copied())
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
struct ModelSlot {
    value: ModelValue,
    writable: bool,
    enumerable: bool,
    configurable: bool,
}
impl ModelSlot {
    fn descriptor(&self) -> PropertyDescriptor {
        PropertyDescriptor::data(
            self.value.value(),
            self.writable,
            self.enumerable,
            self.configurable,
        )
    }
    fn permits(&self, next: &Self) -> bool {
        self.configurable
            || (!next.configurable
                && self.enumerable == next.enumerable
                && (self.writable || (!next.writable && self.value.matches(&next.value.value()))))
    }
}

#[test]
fn complete_descriptors_match_an_independent_model_with_gc_between_operations() {
    let context = AllocationContext::new();
    let array = ArrayObject::new(&context, 0).unwrap();
    let object = array.object();
    let mut model = BTreeMap::<u32, ModelSlot>::new();
    let mut length = 96;
    for index in 0..length {
        let slot = ModelSlot {
            value: ModelValue::Number(f64::from(index)),
            writable: true,
            enumerable: true,
            configurable: true,
        };
        assert!(
            object
                .define_own_property(index.to_string().as_str().into(), slot.descriptor())
                .unwrap()
        );
        model.insert(index, slot);
    }
    let mut seed = 0x7654_3210_u32;
    let steps = if cfg!(miri) { 48 } else { 1024 };
    for step in 0..steps {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let index = if step % 37 == 0 {
            u32::MAX - 1
        } else {
            seed % 128
        };
        let key = PropertyKey::from(index.to_string().as_str());
        match (seed >> 16) % 5 {
            0..=2 => {
                let value = match (seed >> 8) % 7 {
                    0 => ModelValue::Number(f64::NAN),
                    1 => ModelValue::Number(-0.0),
                    2 => ModelValue::Number(f64::INFINITY),
                    3 => ModelValue::Text("mixed"),
                    _ => ModelValue::Number(f64::from(step)),
                };
                let next = ModelSlot {
                    value,
                    writable: seed & 8 != 0,
                    enumerable: seed & 16 != 0,
                    configurable: seed & 32 != 0,
                };
                let expected = model.get(&index).is_none_or(|old| old.permits(&next));
                assert_eq!(
                    object.define_own_property(key, next.descriptor()).unwrap(),
                    expected,
                    "define step {step}"
                );
                if expected {
                    model.insert(index, next);
                    length = length.max(index + 1);
                }
            }
            3 => {
                let expected = model.get(&index).is_none_or(|slot| slot.configurable);
                assert_eq!(
                    object.try_delete(&key).unwrap(),
                    expected,
                    "delete step {step}"
                );
                if expected {
                    model.remove(&index);
                }
            }
            _ => {
                let target = seed % 129;
                let blocked = model
                    .iter()
                    .rev()
                    .find(|(key, slot)| **key >= target && !slot.configurable)
                    .map(|(key, _)| *key);
                let expected = target >= length || blocked.is_none();
                if target < length {
                    length = blocked.map_or(target, |index| index + 1);
                    model.retain(|index, _| *index < length);
                } else {
                    length = target;
                }
                assert_eq!(
                    object
                        .define_own_property(
                            "length".into(),
                            PropertyDescriptor {
                                value: Some(Value::Number(f64::from(target))),
                                ..PropertyDescriptor::default()
                            }
                        )
                        .unwrap(),
                    expected,
                    "shrink step {step}"
                );
            }
        }
        if step % 13 == 0 {
            context.collect_garbage().unwrap();
        }
        assert_eq!(array.length(), length, "length step {step}");
        let expected_keys: Vec<_> = model
            .keys()
            .map(|index| PropertyKey::from(index.to_string().as_str()))
            .chain(["length".into()])
            .collect();
        assert_eq!(
            object.own_property_keys(),
            expected_keys,
            "keys step {step}"
        );
        for (index, expected) in &model {
            let actual = object
                .get_own_property(&index.to_string().as_str().into())
                .unwrap();
            assert_eq!(actual.writable, Some(expected.writable));
            assert_eq!(actual.enumerable, Some(expected.enumerable));
            assert_eq!(actual.configurable, Some(expected.configurable));
            assert!(
                expected.value.matches(actual.value.as_ref().unwrap()),
                "value at {index}, step {step}"
            );
            assert!(
                expected
                    .value
                    .matches(&object.get(&index.to_string().as_str().into()).unwrap())
            );
        }
    }
    drop((array, object));
    assert_eq!(context.collect_garbage().unwrap().retained, 0);
}
