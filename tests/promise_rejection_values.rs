use std::{cell::RefCell, rc::Rc};

use num_bigint::BigInt;
use temiya::{
    Engine,
    embedding::{RealmHandle, ScriptBackend},
    runtime::{JsString, Object, PropertyDescriptor, RuntimeError, Symbol, Value},
};

fn evaluate(engine: &Engine, realm: &RealmHandle, source: &str) {
    let script = engine.compile(realm, source).unwrap();
    engine.evaluate(&script).unwrap();
}

fn reasons() -> Vec<Value> {
    vec![
        Value::Undefined,
        Value::Null,
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Number(0.0),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::String(JsString::from_code_units(vec![0, 0xd800])),
        Value::BigInt(BigInt::from(123456789)),
        Value::Symbol(Symbol::new(Some("reason".into()))),
        Value::Object(Object::new()),
    ]
}

#[test]
fn javascript_reactions_and_unhandled_hook_preserve_every_value_kind() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        for reason in reasons() {
            let engine = Engine::builder().script_backend(backend).build();
            let realm = engine.create_realm();
            engine
                .set_global(
                    &realm,
                    "reason",
                    engine.value(&realm, reason.clone()).unwrap(),
                )
                .unwrap();
            let received = Rc::new(RefCell::new(Vec::new()));
            let observed = received.clone();
            engine
                .register_global_function(&realm, "capture", move |context, _, arguments| {
                    observed.borrow_mut().push(arguments[0].value().clone());
                    Ok(context.value(Value::Undefined))
                })
                .unwrap();
            let unhandled = Rc::new(RefCell::new(Vec::new()));
            let observed = unhandled.clone();
            engine
                .register_job_error_callback(&realm, move |error| {
                    observed
                        .borrow_mut()
                        .push(error.thrown().map(|value| value.value().clone()));
                })
                .unwrap();
            evaluate(
                &engine,
                &realm,
                "async function fail(){throw reason;} \
                 fail().then(undefined,capture); \
                 fail().then().then(undefined,capture); \
                 async function adopt(value){return value;} \
                 adopt({then:function(resolve,reject){reject(reason);}}).then(undefined,capture); \
                 async function caught(){try{await fail();}catch(value){capture(value);}} \
                 caught();",
            );
            engine.drain_jobs(&realm).unwrap();
            assert_eq!(received.borrow().len(), 4);
            assert!(
                received
                    .borrow()
                    .iter()
                    .all(|value| value.same_value(&reason))
            );
            assert!(unhandled.borrow().is_empty());
            evaluate(&engine, &realm, "fail();");
            engine.drain_jobs(&realm).unwrap();
            let unhandled = unhandled.borrow();
            assert_eq!(unhandled.len(), 1);
            assert!(unhandled[0].as_ref().unwrap().same_value(&reason));
        }
    }
}

#[test]
fn thenable_reject_callback_and_throwing_getter_keep_the_original_object() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let reason = Value::Object(Object::new());
    engine
        .set_global(
            &realm,
            "reason",
            engine.value(&realm, reason.clone()).unwrap(),
        )
        .unwrap();
    let received = Rc::new(RefCell::new(Vec::new()));
    let observed = received.clone();
    engine
        .register_global_function(&realm, "capture", move |context, _, arguments| {
            observed.borrow_mut().push(arguments[0].value().clone());
            Ok(context.value(Value::Undefined))
        })
        .unwrap();
    let unhandled = Rc::new(RefCell::new(Vec::new()));
    let observed = unhandled.clone();
    engine
        .register_job_error_callback(&realm, move |error| {
            observed
                .borrow_mut()
                .push(error.thrown().unwrap().value().clone());
        })
        .unwrap();
    evaluate(
        &engine,
        &realm,
        "async function adopt(value){return value;} \
         adopt({then:function(resolve,reject){reject(reason);}}).then(undefined,capture); \
         adopt({then:function(){throw reason;}}).then(undefined,capture);",
    );
    let thenable = Object::new();
    let thrown = reason.clone();
    thenable
        .define_own_property(
            "then".into(),
            PropertyDescriptor::accessor(
                Some(Object::native_function(move |_, _| {
                    Err(RuntimeError::thrown(thrown.clone()))
                })),
                None,
                true,
                true,
            ),
        )
        .unwrap();
    engine
        .set_global(
            &realm,
            "throwingGetter",
            engine.value(&realm, Value::Object(thenable)).unwrap(),
        )
        .unwrap();
    evaluate(
        &engine,
        &realm,
        "adopt(throwingGetter).then(undefined,capture);",
    );
    engine.drain_jobs(&realm).unwrap();
    assert_eq!(received.borrow().len(), 3);
    assert!(
        received
            .borrow()
            .iter()
            .all(|value| value.same_value(&reason))
    );
    assert!(unhandled.borrow().is_empty());
    evaluate(&engine, &realm, "adopt(throwingGetter);");
    engine.drain_jobs(&realm).unwrap();
    assert_eq!(unhandled.borrow().len(), 1);
    assert!(unhandled.borrow()[0].same_value(&reason));
}

#[test]
#[ignore = "requires the pinned Test262 corpus; see docs/runtime.md"]
fn pinned_test262_async_rejection_values() {
    use std::process::Command;
    use temiya::test262::{TEST262_REVISION, TestCase, TestVariant};

    let corpus = std::env::var("TEST262_DIR").expect("set TEST262_DIR");
    let read = |path: &str| {
        let result = Command::new("git")
            .args([
                "-c",
                &format!("safe.directory={corpus}"),
                "-C",
                &corpus,
                "show",
                &format!("{TEST262_REVISION}:{path}"),
            ])
            .output()
            .unwrap();
        assert!(result.status.success(), "missing pinned source {path}");
        String::from_utf8(result.stdout).unwrap()
    };
    let harness = format!("{}\n{}", read("harness/sta.js"), read("harness/assert.js"));
    let mut passed = 0;
    for name in [
        "evaluation-body-that-throws.js",
        "evaluation-default-that-throws.js",
        "try-throw-finally-throw.js",
        "try-return-finally-throw.js",
    ] {
        let path = format!("test/language/statements/async-function/{name}");
        let case = TestCase::parse(&path, read(&path)).unwrap();
        assert!(case.metadata.flags.iter().any(|flag| flag == "async"));
        assert!(case.metadata.includes.is_empty() && case.metadata.negative.is_none());
        for variant in case.variants() {
            assert_ne!(variant, TestVariant::Module);
            for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
                let engine = Engine::builder().script_backend(backend).build();
                let realm = engine.create_realm();
                let done = Rc::new(RefCell::new(Vec::new()));
                let observed = done.clone();
                engine
                    .register_global_function(&realm, "$DONE", move |context, _, arguments| {
                        observed.borrow_mut().push(
                            arguments
                                .first()
                                .map_or(Value::Undefined, |value| value.value().clone()),
                        );
                        Ok(context.value(Value::Undefined))
                    })
                    .unwrap();
                let unhandled = Rc::new(RefCell::new(Vec::new()));
                let observed = unhandled.clone();
                engine
                    .register_job_error_callback(&realm, move |error| {
                        observed.borrow_mut().push(error)
                    })
                    .unwrap();
                evaluate(&engine, &realm, &harness);
                evaluate(&engine, &realm, &case.source_for(variant));
                engine.drain_jobs(&realm).unwrap();
                assert_eq!(done.borrow().len(), 1, "{path} {variant:?} {backend:?}");
                assert!(
                    matches!(done.borrow()[0], Value::Undefined),
                    "{path} {variant:?} {backend:?}: {:?}",
                    done.borrow()
                );
                assert!(
                    unhandled.borrow().is_empty(),
                    "{path}: {:?}",
                    unhandled.borrow()
                );
                passed += 1;
                println!("PASS {path} {variant:?} {backend:?}");
            }
        }
    }
    assert_eq!(passed, 16);
    println!("Test262 {TEST262_REVISION}: {passed} passed; 0 failed; 0 skipped");
}

#[test]
fn native_javascript_errors_remain_error_objects_in_reactions() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    evaluate(
        &engine,
        &realm,
        "var nativeError=false; async function fail(){return null.x;} \
         fail().then(undefined,function(error){nativeError=error instanceof TypeError;});",
    );
    engine.drain_jobs(&realm).unwrap();
    let script = engine.compile(&realm, "nativeError;").unwrap();
    assert!(
        engine
            .evaluate(&script)
            .unwrap()
            .value()
            .same_value(&Value::Boolean(true))
    );
}
