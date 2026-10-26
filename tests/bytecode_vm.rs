use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, StatementEvaluator, Value},
};

fn evaluate_both(source: &str) -> (Completion, Completion) {
    let program = parse_script(source).unwrap();
    let interpreted = StatementEvaluator::new(Realm::ordinary())
        .evaluate_program(&program)
        .unwrap();
    let bytecode = BytecodeCompiler::compile_program(&program).unwrap();
    let executed = VirtualMachine::execute(&bytecode).unwrap();
    (interpreted, executed)
}

fn assert_same_completion(left: &Completion, right: &Completion) {
    match (left, right) {
        (Completion::Normal(Some(left)), Completion::Normal(Some(right)))
        | (Completion::Return(left), Completion::Return(right))
        | (Completion::Throw(left), Completion::Throw(right)) => {
            assert!(left.same_value(right), "{left:?} != {right:?}");
        }
        (Completion::Normal(None), Completion::Normal(None)) => {}
        _ => panic!("completion mismatch: {left:?} != {right:?}"),
    }
}

#[test]
fn arithmetic_conditionals_and_strings_match_ast_interpreter() {
    for source in [
        "40 + 2;",
        "true ? 6 * 7 : 0;",
        "false ? 0 : 84 / 2;",
        "'left' + 'right';",
        "!(1 === 2);",
    ] {
        let (interpreted, executed) = evaluate_both(source);
        assert_same_completion(&interpreted, &executed);
    }
}

#[test]
fn local_side_effect_order_and_control_flow_match() {
    for source in [
        "let x = 1; (x = x + 1) + (x = x * 10); x;",
        "let x = 0; if (true) { x = 42; } else { x = 1; } x;",
        "let x = 0; let run = true; while (run) { x = x + 1; run = false; } x;",
    ] {
        let (interpreted, executed) = evaluate_both(source);
        assert_same_completion(&interpreted, &executed);
    }
}

#[test]
fn throw_completion_preserves_its_value() {
    let (interpreted, executed) = evaluate_both("throw 'boom';");
    assert_same_completion(&interpreted, &executed);
}

#[test]
fn source_compile_reports_parse_and_unsupported_errors() {
    assert!(BytecodeCompiler::compile("let = ;").is_err());
    let unsupported = BytecodeCompiler::compile("({ answer: 42 });").unwrap_err();
    assert!(
        unsupported
            .message()
            .contains("unsupported bytecode expression")
    );
    assert!(unsupported.span().is_some());
    let unbound = BytecodeCompiler::compile("missing;").unwrap();
    assert_eq!(
        VirtualMachine::execute(&unbound).unwrap_err().kind(),
        temiya::runtime::RuntimeErrorKind::ReferenceError
    );
}

#[test]
fn division_edges_retain_javascript_number_behavior() {
    let bytecode = BytecodeCompiler::compile("1 / 0;").unwrap();
    let Completion::Normal(Some(Value::Number(value))) =
        VirtualMachine::execute(&bytecode).unwrap()
    else {
        panic!("expected number completion")
    };
    assert_eq!(value, f64::INFINITY);
}

fn vm_value(realm: &Realm, source: &str) -> Value {
    let code = BytecodeCompiler::compile(source).unwrap();
    match VirtualMachine::execute_in_realm(&code, realm).unwrap() {
        Completion::Normal(Some(value)) => value,
        other => panic!("unexpected completion: {other:?}"),
    }
}

#[test]
fn call_frames_capture_method_before_nested_arguments_and_preserve_receiver() {
    use std::{cell::RefCell, rc::Rc};
    use temiya::runtime::{Object, PropertyDescriptor};
    let realm = Realm::ordinary();
    let events = Rc::new(RefCell::new(vec![]));
    let receiver = Object::new();
    let expected = receiver.clone();
    let log = events.clone();
    let method = Object::native_function(move |this, args| {
        assert!(this.same_value(&Value::Object(expected.clone())));
        assert!(args[0].same_value(&Value::Number(21.0)));
        assert!(args[1].same_value(&Value::Number(21.0)));
        log.borrow_mut().push("call");
        Ok(Value::Number(42.0))
    });
    let log = events.clone();
    let getter = Object::native_function(move |_, _| {
        log.borrow_mut().push("get");
        Ok(Value::Object(method.clone()))
    });
    receiver
        .define_own_property(
            "method".into(),
            PropertyDescriptor::accessor(Some(getter), None, true, true),
        )
        .unwrap();
    let log = events.clone();
    let argument = Object::native_function(move |this, _| {
        assert!(this.same_value(&Value::Undefined));
        log.borrow_mut().push("arg");
        Ok(Value::Number(21.0))
    });
    realm
        .global_object()
        .create_data_property("receiver".into(), Value::Object(receiver.clone()))
        .unwrap();
    realm
        .global_object()
        .create_data_property("argument".into(), Value::Object(argument))
        .unwrap();
    for source in [
        "(receiver.method)(argument(), argument());",
        "receiver.method?.(argument(), argument());",
        "(receiver?.method)?.(argument(), argument());",
        "receiver?.method?.(argument(), argument());",
    ] {
        assert!(vm_value(&realm, source).same_value(&Value::Number(42.0)));
        assert_eq!(*events.borrow(), ["get", "arg", "arg", "call"]);
        events.borrow_mut().clear();
    }
    // Break the deliberately cyclic test getter capture.
    receiver.try_delete(&"method".into()).unwrap();
}

#[test]
fn construct_frame_passes_arguments_and_original_new_target() {
    use std::{cell::RefCell, rc::Rc};
    use temiya::runtime::Object;
    let realm = Realm::ordinary();
    let recorded = Rc::new(RefCell::new(None));
    let captured = recorded.clone();
    let constructor = Object::native_constructor(move |args, target| {
        assert!(args[0].same_value(&Value::Number(42.0)));
        *captured.borrow_mut() = Some(target);
        let result = Object::new();
        result.create_data_property("answer".into(), args[0].clone())?;
        Ok(result)
    });
    realm
        .global_object()
        .create_data_property("Ctor".into(), Value::Object(constructor.clone()))
        .unwrap();
    assert!(vm_value(&realm, "new Ctor((Ctor = 0, 42)).answer;").same_value(&Value::Number(42.0)));
    assert!(
        Value::Object(recorded.borrow_mut().take().unwrap())
            .same_value(&Value::Object(constructor))
    );
}

#[test]
fn call_frame_argument_failure_and_reentry_leave_vm_reusable() {
    use temiya::runtime::{Object, RuntimeErrorKind};
    let realm = Realm::ordinary();
    let inner = BytecodeCompiler::compile("40 + 2;").unwrap();
    let callback = Object::native_function(move |_, _| match VirtualMachine::execute(&inner)? {
        Completion::Normal(Some(value)) => Ok(value),
        _ => panic!("unexpected nested result"),
    });
    realm
        .global_object()
        .create_data_property("callback".into(), Value::Object(callback))
        .unwrap();
    for source in [
        "1(callback());",
        "new callback(missing);",
        "callback(missing);",
    ] {
        let code = BytecodeCompiler::compile(source).unwrap();
        let error = VirtualMachine::execute_in_realm(&code, &realm).unwrap_err();
        assert_eq!(
            error.kind(),
            if source.contains("missing") {
                RuntimeErrorKind::ReferenceError
            } else {
                RuntimeErrorKind::TypeError
            }
        );
        assert!(vm_value(&realm, "callback();").same_value(&Value::Number(42.0)));
    }
}

#[test]
fn vm_calls_existing_javascript_functions_through_real_object_call() {
    let realm = Realm::ordinary();
    StatementEvaluator::new(realm.clone())
        .evaluate_program(&parse_script("function twice(x) { return x * 2; }").unwrap())
        .unwrap();
    assert!(vm_value(&realm, "twice(21);").same_value(&Value::Number(42.0)));
    for source in [
        "Math.abs(0 - 42);",
        "String(42);",
        "new Number(42).valueOf();",
    ] {
        let interpreted = StatementEvaluator::new(realm.clone())
            .evaluate_program(&parse_script(source).unwrap())
            .unwrap();
        let code = BytecodeCompiler::compile(source).unwrap();
        assert_same_completion(
            &interpreted,
            &VirtualMachine::execute_in_realm(&code, &realm).unwrap(),
        );
    }
}

#[test]
fn recursive_javascript_call_overflow_restores_call_budget() {
    use temiya::runtime::RuntimeErrorKind;
    let realm = Realm::ordinary();
    StatementEvaluator::new(realm.clone())
        .evaluate_program(
            &parse_script("function recur() { return recur(); } function answer() { return 42; }")
                .unwrap(),
        )
        .unwrap();
    let code = BytecodeCompiler::compile("recur();").unwrap();
    for _ in 0..2 {
        assert_eq!(
            VirtualMachine::execute_in_realm(&code, &realm)
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::RangeError
        );
        assert!(vm_value(&realm, "answer();").same_value(&Value::Number(42.0)));
    }
}

#[test]
fn pending_callee_roots_drop_after_argument_throw_and_normal_return() {
    use std::rc::Rc;
    use temiya::runtime::Object;
    for source in ["f(f = 0, missing);", "f(f = 0);"] {
        let realm = Realm::ordinary();
        let marker = Rc::new(());
        let weak = Rc::downgrade(&marker);
        let function = Object::native_function(move |_, _| {
            let _keep_alive = &marker;
            Ok(Value::Number(42.0))
        });
        realm
            .global_object()
            .create_data_property("f".into(), Value::Object(function))
            .unwrap();
        let code = BytecodeCompiler::compile(source).unwrap();
        let result = VirtualMachine::execute_in_realm(&code, &realm);
        assert_eq!(result.is_err(), source.contains("missing"));
        assert!(weak.upgrade().is_none(), "callee retained after {source}");
    }
}

#[test]
fn optional_calls_propagate_short_circuits_and_preserve_grouped_receivers() {
    let bootstrap = parse_script("var nil=null; var count=0; var receiver={answer:42,method:function(x){count=count+1;return this.answer+x;},absent:null}; function factory(){count=count+1;return receiver;} function nothing(){count=count+1;return null;} function identity(x){return x;}").unwrap();
    for source in [
        "nil?.method(count=count+1).answer;",
        "nil?.method?.(count=count+1)();",
        "receiver.absent?.(count=count+1).answer;",
        "receiver.method?.(0);",
        "(receiver?.method)?.(0);",
        "(receiver?.method)(0);",
        "(receiver.method)?.(identity(0));",
        "factory()?.method?.(0);",
        "nothing()?.method().answer;",
        "identity?.(receiver)?.method(0);",
        "receiver?.method(0)?.missing?.(count=count+1);",
        "delete nil?.method().answer;",
        "delete receiver?.method(0).missing;",
        "nil?.method(missing);",
        "(nil?.method)(count=count+1);",
        "(nil?.method)?.(count=count+1);",
        "(nil?.method).answer;",
        "receiver.absent?.().answer;",
        "receiver.method?.(missing);",
        "receiver.answer?.(count=count+1);",
        "nothing()?.().answer;",
        "false?.(count=count+1);",
        "identity((nil?.method)?.(count=count+1));",
    ] {
        let ast_realm = Realm::ordinary();
        let vm_realm = Realm::ordinary();
        for realm in [&ast_realm, &vm_realm] {
            StatementEvaluator::new(realm.clone())
                .evaluate_program(&bootstrap)
                .unwrap();
        }
        let expected = StatementEvaluator::new(ast_realm.clone())
            .evaluate_program(&parse_script(source).unwrap());
        let code =
            BytecodeCompiler::compile(source).unwrap_or_else(|error| panic!("{source}: {error}"));
        let actual = VirtualMachine::execute_in_realm(&code, &vm_realm);
        match (expected, actual) {
            (Ok(left), Ok(right)) => assert_same_completion(&left, &right),
            (Err(left), Err(right)) => assert_eq!(left.kind(), right.kind(), "{source}"),
            pair => panic!("{source}: {pair:?}"),
        }
        let key = temiya::runtime::PropertyKey::from("count");
        assert!(
            ast_realm
                .global_object()
                .get(&key)
                .unwrap()
                .same_value(&vm_realm.global_object().get(&key).unwrap()),
            "argument/call count differs for {source}"
        );
    }
}
