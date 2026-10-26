use std::{cell::Cell, rc::Rc};

use temiya::{
    Engine,
    embedding::{EmbedError, ExceptionKind},
    runtime::Value,
};

#[test]
fn builder_owns_host_state_and_configuration() {
    let dropped = Rc::new(Cell::new(false));
    struct HostState(Rc<Cell<bool>>);
    impl Drop for HostState {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let engine = Engine::builder()
        .development_checks(true)
        .host_state(HostState(dropped.clone()))
        .build();
    assert!(engine.options().development_checks);
    assert!(engine.host_state::<HostState>().is_some());
    assert!(engine.host_state::<String>().is_none());
    drop(engine);
    assert!(dropped.get());
}

#[test]
fn compile_and_evaluate_return_owned_value_handles() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine.compile(&realm, "40 + 2;").unwrap();
    let value = engine.evaluate(&script).unwrap();
    assert!(matches!(value.value(), Value::Number(42.0)));
    let second = engine.evaluate(&script).unwrap();
    assert!(value.shares_realm_with(&second));
}

#[test]
fn compile_and_runtime_failures_are_distinct() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    assert!(matches!(
        engine.compile(&realm, "let = ;"),
        Err(EmbedError::Compile(_))
    ));
    let thrown = engine.compile(&realm, "throw 7;").unwrap();
    let exception = engine.evaluate(&thrown).unwrap_err();
    assert_eq!(exception.kind(), ExceptionKind::ThrownValue);
    assert!(matches!(
        exception.thrown().unwrap().value(),
        Value::Number(7.0)
    ));
}

#[test]
fn cross_engine_handles_are_rejected() {
    let first = Engine::default();
    let second = Engine::default();
    let realm = first.create_realm();
    assert!(matches!(
        second.compile(&realm, "1;"),
        Err(EmbedError::EngineMismatch)
    ));
    let script = first.compile(&realm, "1;").unwrap();
    assert_eq!(
        second.evaluate(&script).unwrap_err().kind(),
        ExceptionKind::Engine
    );
}
