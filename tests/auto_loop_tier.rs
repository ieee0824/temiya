use std::{cell::Cell, rc::Rc};
use temiya::{
    Engine,
    embedding::{ExceptionKind, ScriptBackend},
    runtime::Value,
};

fn run(backend: ScriptBackend, body: &str, call: &str) -> Value {
    let engine = Engine::builder().script_backend(backend).build();
    let realm = engine.create_realm();
    let setup = format!("var setup_object = {{}}; function f(n) {{ {body} }}");
    engine
        .evaluate(&engine.compile(&realm, &setup).unwrap())
        .unwrap();
    engine
        .evaluate(&engine.compile(&realm, call).unwrap())
        .unwrap()
        .value()
        .clone()
}

#[test]
fn auto_function_tier_matches_ast_on_nested_control_and_captures() {
    for (body, call, expected) in [
        (
            "var s=0; outer:for(var i=0;i<n;i++){ for(var j=0;j<4;j++){if(j===2)continue outer;s+=j;} } return s;",
            "f(5);",
            5.0,
        ),
        (
            "var x=n; var f; while(x<42){ f=()=>x; x++; } return f();",
            "f(40);",
            42.0,
        ),
        (
            "var n2=n; do{ n2++; if(n2===42) return n2; }while(n2<50); return 0;",
            "f(40);",
            42.0,
        ),
        (
            "var x=n; while(x<10){x++;} try{return x;}finally{}",
            "f(2);",
            10.0,
        ),
    ] {
        for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
            assert!(
                run(backend, body, call).same_value(&Value::Number(expected)),
                "{backend:?}: {body}"
            );
        }
    }
}

#[test]
fn thrown_vm_body_is_not_retried() {
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Auto)
        .build();
    let realm = engine.create_realm();
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    engine
        .register_global_function(&realm, "hit", move |context, _, _| {
            observed.set(observed.get() + 1);
            Ok(context.value(Value::Undefined))
        })
        .unwrap();
    engine
        .evaluate(
            &engine
                .compile(
                    &realm,
                    "var setup_object={}; function f(){while(true){hit();throw 7;}}",
                )
                .unwrap(),
        )
        .unwrap();
    let error = engine
        .evaluate(&engine.compile(&realm, "f();").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), ExceptionKind::ThrownValue);
    assert!(matches!(
        error.thrown().unwrap().value(),
        Value::Number(7.0)
    ));
    assert_eq!(count.get(), 1);
}
