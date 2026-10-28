use std::{cell::RefCell, rc::Rc};

use temiya::{
    parser::parse_expression,
    runtime::{
        Environment, ExpressionEvaluator, JsString, Object, PropertyDescriptor, PropertyKey, Realm,
        RuntimeErrorKind, Value,
    },
};

fn evaluator() -> (ExpressionEvaluator, Environment) {
    let allocation_context = temiya::runtime::AllocationContext::new();
    evaluator_in(&allocation_context)
}

fn evaluator_in(
    allocation_context: &temiya::runtime::AllocationContext,
) -> (ExpressionEvaluator, Environment) {
    let realm = Realm::ordinary_in(allocation_context);
    let environment = Environment::declarative(realm.global_environment());
    (
        ExpressionEvaluator::with_environment(realm, environment.clone(), Value::Undefined, false),
        environment,
    )
}

fn bind(environment: &Environment, name: &str, value: Value) {
    let name = JsString::from(name);
    environment
        .create_mutable_binding(name.clone(), true)
        .unwrap();
    environment.initialize_binding(&name, value).unwrap();
}

fn evaluate(evaluator: &ExpressionEvaluator, source: &str) -> Value {
    evaluator
        .evaluate(&parse_expression(source).unwrap())
        .unwrap()
}

#[test]
fn evaluates_literals_operators_and_assignment_references() {
    let (evaluator, environment) = evaluator();
    bind(&environment, "answer", Value::Number(1.0));

    assert!(evaluate(&evaluator, "answer += 6 * 7").same_value(&Value::Number(43.0)));
    assert!(evaluate(&evaluator, "answer++").same_value(&Value::Number(43.0)));
    assert!(evaluate(&evaluator, "answer").same_value(&Value::Number(44.0)));
    assert!(evaluate(&evaluator, "1n + 2n").same_value(&Value::BigInt(3.into())));
    assert!(evaluate(&evaluator, "'a' + 2").same_value(&Value::String(JsString::from("a2"))));
}

#[test]
fn string_addition_preserves_host_conversion_order_and_abrupt_completion() {
    let (evaluator, environment) = evaluator();
    let allocation_context = environment.allocation_context().clone();
    let events = Rc::new(RefCell::new(Vec::new()));
    for (name, text) in [("left", "a"), ("right", "b")] {
        let object = Object::new_in(&allocation_context);
        let events = events.clone();
        object.set_to_primitive_hook(move |_| {
            events.borrow_mut().push(name);
            Ok(Value::String(text.into()))
        });
        bind(&environment, name, Value::Object(object));
    }
    assert!(evaluate(&evaluator, "left + right").same_value(&Value::String("ab".into())));
    assert_eq!(&*events.borrow(), &["left", "right"]);
    events.borrow_mut().clear();
    let failure = Object::new_in(&allocation_context);
    let captured = events.clone();
    failure.set_to_primitive_hook(move |_| {
        captured.borrow_mut().push("failure");
        Err(temiya::runtime::RuntimeError::type_error(
            "conversion failed",
        ))
    });
    bind(&environment, "failure", Value::Object(failure));
    assert!(
        evaluator
            .evaluate(&parse_expression("failure + right").unwrap())
            .is_err()
    );
    assert_eq!(&*events.borrow(), &["failure"]);
}

#[test]
fn delays_computed_key_coercion_until_after_assignment_rhs() {
    let (evaluator, environment) = evaluator();
    let allocation_context = environment.allocation_context().clone();
    let events = Rc::new(RefCell::new(Vec::new()));
    let target = Object::new_in(&allocation_context);
    let key = Object::new_in(&allocation_context);
    let key_events = events.clone();
    key.set_to_primitive_hook(move |_| {
        key_events.borrow_mut().push("key");
        Ok(Value::String(JsString::from("value")))
    });
    let rhs_events = events.clone();
    let rhs = Object::native_function_in(&allocation_context, move |_, _| {
        rhs_events.borrow_mut().push("rhs");
        Ok(Value::Number(9.0))
    });
    bind(&environment, "target", Value::Object(target.clone()));
    bind(&environment, "key", Value::Object(key));
    bind(&environment, "rhs", Value::Object(rhs));

    evaluate(&evaluator, "target[key] = rhs()");

    assert_eq!(&*events.borrow(), &["rhs", "key"]);
    assert!(
        target
            .get(&PropertyKey::from("value"))
            .unwrap()
            .same_value(&Value::Number(9.0))
    );
}

#[test]
fn preserves_receiver_and_left_to_right_call_order() {
    let (evaluator, environment) = evaluator();
    let allocation_context = environment.allocation_context().clone();
    let receiver = Object::new_in(&allocation_context);
    receiver
        .create_data_property("marker".into(), Value::Number(3.0))
        .unwrap();
    let events = Rc::new(RefCell::new(Vec::new()));
    let method_events = events.clone();
    let method = Object::native_function_in(&allocation_context, move |this, arguments| {
        method_events
            .borrow_mut()
            .push(arguments[0].to_number().unwrap() as i32);
        let Value::Object(this) = this else {
            panic!("missing receiver")
        };
        this.get(&PropertyKey::from("marker"))
    });
    receiver
        .create_data_property("method".into(), Value::Object(method))
        .unwrap();
    let argument_events = events.clone();
    let argument = Object::native_function_in(&allocation_context, move |_, _| {
        argument_events.borrow_mut().push(1);
        Ok(Value::Number(2.0))
    });
    bind(&environment, "object", Value::Object(receiver));
    bind(&environment, "argument", Value::Object(argument));

    assert!(evaluate(&evaluator, "object.method(argument())").same_value(&Value::Number(3.0)));
    assert_eq!(&*events.borrow(), &[1, 2]);
}

#[test]
fn optional_chain_short_circuits_a_contiguous_chain_only() {
    let (evaluator, environment) = evaluator();
    bind(&environment, "missing", Value::Null);

    assert!(evaluate(&evaluator, "missing?.property.call()").same_value(&Value::Undefined));
    let error = evaluator
        .evaluate(&parse_expression("(missing?.property).call()").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::TypeError);
}

#[test]
fn accessor_getvalue_putvalue_and_strict_failures_are_observable() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&allocation_context);
    let environment = Environment::declarative(realm.global_environment());
    let stored = Rc::new(RefCell::new(Value::Number(1.0)));
    let getter_value = stored.clone();
    let getter = Object::native_function_in(&allocation_context, move |_, _| {
        Ok(getter_value.borrow().clone())
    });
    let setter_value = stored.clone();
    let setter = Object::native_function_in(&allocation_context, move |_, arguments| {
        *setter_value.borrow_mut() = arguments[0].clone();
        Ok(Value::Undefined)
    });
    let object = Object::new_in(&allocation_context);
    object
        .define_own_property(
            "value".into(),
            PropertyDescriptor::accessor(Some(getter), Some(setter), true, true),
        )
        .unwrap();
    bind(&environment, "object", Value::Object(object));
    let evaluator =
        ExpressionEvaluator::with_environment(realm, environment, Value::Undefined, true);

    assert!(evaluate(&evaluator, "object.value = 8").same_value(&Value::Number(8.0)));
    assert!(evaluate(&evaluator, "object.value").same_value(&Value::Number(8.0)));
    let error = evaluator
        .evaluate(&parse_expression("unknown = 1").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::ReferenceError);
}

#[test]
fn logical_operators_do_not_evaluate_an_unselected_operand() {
    let (evaluator, environment) = evaluator();
    let allocation_context = environment.allocation_context().clone();
    let calls = Rc::new(RefCell::new(0));
    let observed = calls.clone();
    bind(
        &environment,
        "sideEffect",
        Value::Object(Object::native_function_in(
            &allocation_context,
            move |_, _| {
                *observed.borrow_mut() += 1;
                Ok(Value::Boolean(true))
            },
        )),
    );

    assert!(evaluate(&evaluator, "false && sideEffect()").same_value(&Value::Boolean(false)));
    assert!(evaluate(&evaluator, "true || sideEffect()").same_value(&Value::Boolean(true)));
    assert!(evaluate(&evaluator, "0 ?? sideEffect()").same_value(&Value::Number(0.0)));
    assert_eq!(*calls.borrow(), 0);
}

#[test]
fn builds_array_object_template_and_constructor_results() {
    let (evaluator, environment) = evaluator();
    let allocation_context = environment.allocation_context().clone();
    let constructor = Object::native_constructor_in(&allocation_context, |arguments, target| {
        let object = Object::new_in(target.allocation_context());
        object.create_data_property("value".into(), arguments[0].clone())?;
        Ok(object)
    });
    bind(&environment, "Constructor", Value::Object(constructor));

    let array = evaluate(&evaluator, "[1,,...['x']]");
    let Value::Object(array) = array else {
        panic!("array literal did not create an object")
    };
    assert!(
        array
            .get(&PropertyKey::from("length"))
            .unwrap()
            .same_value(&Value::Number(3.0))
    );
    assert!(
        array
            .get(&PropertyKey::from("1"))
            .unwrap()
            .same_value(&Value::Undefined)
    );
    assert!(
        array
            .get(&PropertyKey::from("2"))
            .unwrap()
            .same_value(&Value::String(JsString::from("x")))
    );

    let object = evaluate(&evaluator, "({answer: 42})");
    let Value::Object(object) = object else {
        panic!("object literal did not create an object")
    };
    assert!(
        object
            .get(&PropertyKey::from("answer"))
            .unwrap()
            .same_value(&Value::Number(42.0))
    );
    assert!(
        evaluate(&evaluator, "`value=${2 + 3}`")
            .same_value(&Value::String(JsString::from("value=5")))
    );
    assert!(evaluate(&evaluator, "new Constructor(4).value").same_value(&Value::Number(4.0)));
}

#[test]
fn invalid_call_and_construct_evaluate_arguments_before_type_errors() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for source in [
        "target(mark = 42)",
        "new target(mark = 42)",
        "target?.(mark = 42)",
    ] {
        for target in [
            Value::Number(1.0),
            Value::Object(Object::new_in(&allocation_context)),
        ] {
            let (evaluator, environment) = evaluator_in(&allocation_context);
            bind(&environment, "target", target);
            bind(&environment, "mark", Value::Number(0.0));
            let error = evaluator
                .evaluate(&parse_expression(source).unwrap())
                .unwrap_err();
            assert_eq!(error.kind(), RuntimeErrorKind::TypeError);
            assert!(evaluate(&evaluator, "mark").same_value(&Value::Number(42.0)));
        }
    }
    let (evaluator, environment) = evaluator();
    bind(&environment, "target", Value::Null);
    bind(&environment, "mark", Value::Number(0.0));
    assert!(evaluate(&evaluator, "target?.(mark = 42)").same_value(&Value::Undefined));
    assert!(evaluate(&evaluator, "mark").same_value(&Value::Number(0.0)));
}

#[test]
fn argument_abrupt_completion_precedes_invalid_callee_error() {
    let (evaluator, environment) = evaluator();
    bind(&environment, "target", Value::Number(1.0));
    for source in [
        "target(missing)",
        "new target(missing)",
        "target?.(missing)",
    ] {
        let error = evaluator
            .evaluate(&parse_expression(source).unwrap())
            .unwrap_err();
        assert_eq!(error.kind(), RuntimeErrorKind::ReferenceError);
    }
}
