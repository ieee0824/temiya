use std::rc::Rc;

use temiya::runtime::{AllocationContext, Environment, JsString, Object, RuntimeErrorKind, Value};

fn number_bits(environment: &Environment, name: &JsString) -> u64 {
    let Value::Number(value) = environment.get_binding_value(name, true).unwrap() else {
        panic!("numeric binding required")
    };
    value.to_bits()
}

#[test]
fn numeric_updates_preserve_bits_and_general_write_validation() {
    let context = AllocationContext::new();
    let environment = Environment::declarative_in(&context);
    let name = JsString::from("value");
    environment
        .create_mutable_binding(name.clone(), false)
        .unwrap();
    assert_eq!(
        environment
            .set_mutable_binding(&name, Value::Number(0.0), false)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::ReferenceError
    );
    environment
        .initialize_binding(&name, Value::Number(0.0))
        .unwrap();
    let bits = [
        0,
        1 << 63,
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        0x7ff8_0000_0000_0042,
        0xfff8_0000_0000_0042,
        1,
        f64::MAX.to_bits(),
    ];
    for strict in [false, true] {
        for bits in bits {
            environment
                .set_mutable_binding(&name, Value::Number(f64::from_bits(bits)), strict)
                .unwrap();
            context.collect_garbage().unwrap();
            assert_eq!(number_bits(&environment, &name), bits);
        }
    }

    let foreign = AllocationContext::new();
    assert_eq!(
        environment
            .set_mutable_binding(&name, Value::Object(Object::new_in(&foreign)), true)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(number_bits(&environment, &name), f64::MAX.to_bits());
    assert_eq!(foreign.collect_garbage().unwrap().retained, 0);

    let marker = Rc::new(());
    let captured = marker.clone();
    let child = Object::native_function_in(&context, move |_, _| {
        let _ = &captured;
        Ok(Value::Number(42.0))
    });
    environment
        .set_mutable_binding(&name, Value::Object(child), true)
        .unwrap();
    context.collect_garbage().unwrap();
    assert_eq!(Rc::strong_count(&marker), 2);
    {
        let Value::Object(child) = environment.get_binding_value(&name, true).unwrap() else {
            panic!("stored child required")
        };
        assert!(
            child
                .call(Value::Undefined, &[])
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
    }
    environment
        .set_mutable_binding(&name, Value::Number(-0.0), true)
        .unwrap();
    context.collect_garbage().unwrap();
    assert_eq!(Rc::strong_count(&marker), 1);
    assert_eq!(number_bits(&environment, &name), (-0.0_f64).to_bits());
    environment
        .set_mutable_binding(&name, Value::String("leaf".into()), true)
        .unwrap();
    assert!(
        environment
            .get_binding_value(&name, true)
            .unwrap()
            .same_value(&Value::String("leaf".into()))
    );
    environment
        .set_mutable_binding(&name, Value::Number(1.0), true)
        .unwrap();
    environment
        .set_mutable_binding(&name, Value::Number(-0.0), true)
        .unwrap();
    assert_eq!(number_bits(&environment, &name), (-0.0_f64).to_bits());

    for (declared_strict, write_strict) in [(true, false), (false, true), (false, false)] {
        let key = JsString::from(format!("constant{declared_strict}{write_strict}").as_str());
        environment
            .create_immutable_binding(key.clone(), declared_strict)
            .unwrap();
        environment
            .initialize_binding(&key, Value::Number(-0.0))
            .unwrap();
        let result = environment.set_mutable_binding(&key, Value::Number(0.0), write_strict);
        if declared_strict || write_strict {
            assert_eq!(result.unwrap_err().kind(), RuntimeErrorKind::TypeError);
        } else {
            result.unwrap();
        }
        assert_eq!(number_bits(&environment, &key), (-0.0_f64).to_bits());
    }
    drop(environment);
    assert_eq!(context.collect_garbage().unwrap().retained, 0);
}
