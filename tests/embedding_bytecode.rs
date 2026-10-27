use std::{cell::Cell, rc::Rc};
use temiya::{
    Engine,
    embedding::{EmbedError, ExceptionKind, ScriptBackend},
    runtime::Value,
};

#[test]
fn default_engines_select_whole_script_backends_before_observable_effects() {
    for engine in [
        Engine::default(),
        Engine::new(Default::default()),
        Engine::builder().build(),
    ] {
        let realm = engine.create_realm();
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        engine
            .register_global_function(&realm, "host", move |context, _, _| {
                observed.set(observed.get() + 1);
                Ok(context.value(Value::Undefined))
            })
            .unwrap();
        let script = engine
            .compile(
                &realm,
                "function f(n){var s=0;for(var i=0;i<n;i++){s+=i;}return s;}f(7)*2;",
            )
            .unwrap();
        assert_eq!(script.backend(), ScriptBackend::Bytecode);
        assert!(
            engine
                .evaluate(&script)
                .unwrap()
                .value()
                .same_value(&Value::Number(42.0))
        );
        let fallback = engine
            .compile(&realm, "host();try{throw 17;}catch(e){e;}")
            .unwrap();
        assert_eq!(fallback.backend(), ScriptBackend::Ast);
        assert_eq!(calls.get(), 0);
        assert!(
            engine
                .evaluate(&fallback)
                .unwrap()
                .value()
                .same_value(&Value::Number(17.0))
        );
        assert_eq!(calls.get(), 1);
        let throwing = engine.compile(&realm, "host();throw 17;").unwrap();
        assert_eq!(throwing.backend(), ScriptBackend::Bytecode);
        assert_eq!(
            engine.evaluate(&throwing).unwrap_err().kind(),
            ExceptionKind::ThrownValue
        );
        assert_eq!(
            calls.get(),
            2,
            "runtime errors must never retry through AST"
        );
    }
    let ast = Engine::builder().script_backend(ScriptBackend::Ast).build();
    let realm = ast.create_realm();
    assert_eq!(
        ast.compile(&realm, "40+2;").unwrap().backend(),
        ScriptBackend::Ast
    );
}

#[test]
fn engine_executes_verified_bytecode_in_the_realm() {
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Bytecode)
        .build();
    let realm = engine.create_realm();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    engine
        .register_global_function(&realm, "host", move |context, _, args| {
            observed.set(observed.get() + 1);
            Ok(context.value(args[0].value().clone()))
        })
        .unwrap();
    let program = engine
        .compile(
            &realm,
            "var total = 0; for (var i = 0; i < 7; i++) total += i; host(total * 2);",
        )
        .unwrap();
    assert_eq!(program.backend(), ScriptBackend::Bytecode);
    assert!(matches!(
        engine.evaluate(&program).unwrap().value(),
        Value::Number(42.0)
    ));
    assert_eq!(calls.get(), 1);
    let global = engine.compile(&realm, "total;").unwrap();
    assert!(matches!(
        engine.evaluate(&global).unwrap().value(),
        Value::Number(21.0)
    ));
    let throw = engine.compile(&realm, "throw 7;").unwrap();
    let error = engine.evaluate(&throw).unwrap_err();
    assert_eq!(error.kind(), ExceptionKind::ThrownValue);
    assert!(matches!(
        error.thrown().unwrap().value(),
        Value::Number(7.0)
    ));
}

#[test]
fn unsupported_bytecode_is_rejected_before_effects_and_auto_selects_ast() {
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Bytecode)
        .build();
    let realm = engine.create_realm();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    engine
        .register_global_function(&realm, "host", move |context, _, _| {
            observed.set(observed.get() + 1);
            Ok(context.value(Value::Undefined))
        })
        .unwrap();
    let error = engine
        .compile(&realm, "host(); try { 42; } finally {}")
        .unwrap_err();
    assert!(matches!(error, EmbedError::Bytecode(_)));
    assert_eq!(calls.get(), 0);
    let auto = Engine::builder()
        .script_backend(ScriptBackend::Auto)
        .build();
    let realm = auto.create_realm();
    let script = auto.compile(&realm, "try { 42; } finally {}").unwrap();
    assert_eq!(script.backend(), ScriptBackend::Ast);
    assert!(matches!(
        auto.evaluate(&script).unwrap().value(),
        Value::Number(42.0)
    ));
    let script = auto.compile(&realm, "40 + 2;").unwrap();
    assert_eq!(script.backend(), ScriptBackend::Bytecode);
    assert!(matches!(
        auto.evaluate(&script).unwrap().value(),
        Value::Number(42.0)
    ));
}
