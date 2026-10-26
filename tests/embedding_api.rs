use std::{cell::Cell, cell::RefCell, rc::Rc};

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

#[test]
fn native_functions_convert_values_and_isolate_panics() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    engine
        .register_global_function(&realm, "double", |context, _, arguments| {
            let Value::Number(value) = arguments[0].value() else {
                return Err(temiya::embedding::HostError::type_error("number required"));
            };
            Ok(context.value(Value::Number(value * 2.0)))
        })
        .unwrap();
    let script = engine.compile(&realm, "double(21);").unwrap();
    assert!(matches!(
        engine.evaluate(&script).unwrap().value(),
        Value::Number(42.0)
    ));

    engine
        .register_global_function(&realm, "invoke", |context, _, arguments| {
            let Value::Object(function) = arguments[0].value() else {
                return Err(temiya::embedding::HostError::type_error(
                    "function required",
                ));
            };
            function
                .call(Value::Undefined, &[])
                .map(|value| context.value(value))
                .map_err(|error| temiya::embedding::HostError::type_error(error.to_string()))
        })
        .unwrap();
    let script = engine.compile(&realm, "invoke(() => 42);").unwrap();
    assert!(matches!(
        engine.evaluate(&script).unwrap().value(),
        Value::Number(42.0)
    ));

    engine
        .register_global_function(&realm, "panicHost", |_, _, _| panic!("host bug"))
        .unwrap();
    let script = engine.compile(&realm, "panicHost();").unwrap();
    let error = engine.evaluate(&script).unwrap_err();
    assert_eq!(error.kind(), ExceptionKind::Engine);
    assert_eq!(error.message(), "host function panicked");
}

#[test]
fn host_objects_reject_foreign_values_and_install_globals() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let answer = engine.value(&realm, Value::Number(42.0)).unwrap();
    let object = engine.create_object(&realm, [("answer", answer)]).unwrap();
    engine.set_global(&realm, "host", object).unwrap();
    let script = engine.compile(&realm, "host.answer;").unwrap();
    assert!(matches!(
        engine.evaluate(&script).unwrap().value(),
        Value::Number(42.0)
    ));

    let foreign_engine = Engine::default();
    let foreign_realm = foreign_engine.create_realm();
    let foreign = foreign_engine
        .value(&foreign_realm, Value::Undefined)
        .unwrap();
    assert!(matches!(
        engine.create_object(&realm, [("bad", foreign)]),
        Err(EmbedError::EngineMismatch)
    ));
}

#[test]
fn module_resolver_supports_graphs_and_isolates_panics() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let module = engine
        .load_module(&realm, "entry", |specifier| match specifier {
            "entry" => Ok("import { answer } from 'dep'; answer;".into()),
            "dep" => Ok("export const answer = 42;".into()),
            _ => Err(temiya::embedding::HostError::reference_error("missing")),
        })
        .unwrap();
    engine.evaluate_module(&module).unwrap();

    let error = engine
        .load_module(&realm, "panic", |_| -> Result<String, _> {
            panic!("resolver bug")
        })
        .unwrap_err();
    assert!(matches!(error, EmbedError::Host(_)));
}

#[test]
fn host_jobs_are_fifo_and_panics_become_exceptions() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let order = Rc::new(RefCell::new(Vec::new()));
    for value in [1, 2] {
        let observed = order.clone();
        engine
            .enqueue_job(&realm, move || {
                observed.borrow_mut().push(value);
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(engine.drain_jobs(&realm).unwrap(), 2);
    assert_eq!(&*order.borrow(), &[1, 2]);

    engine
        .enqueue_job(&realm, || -> Result<(), _> { panic!("job bug") })
        .unwrap();
    let error = engine.drain_jobs(&realm).unwrap_err();
    assert_eq!(error.kind(), ExceptionKind::Engine);
    assert_eq!(error.message(), "host job panicked");
}
