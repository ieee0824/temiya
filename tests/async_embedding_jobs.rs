use temiya::{
    Engine,
    embedding::{RealmHandle, ScriptBackend},
    runtime::Value,
};

fn number(engine: &Engine, realm: &RealmHandle, source: &str) -> f64 {
    let script = engine.compile(realm, source).unwrap();
    let value = engine.evaluate(&script).unwrap();
    let Value::Number(number) = value.value() else {
        panic!("expected number, got {:?}", value.value());
    };
    *number
}

#[test]
fn javascript_async_reactions_wait_for_the_public_microtask_checkpoint() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        assert_eq!(
            number(
                &engine,
                &realm,
                "async function seed(){return 1;} var observed=0; \
                 seed().then(function(value){observed=value;}); observed;",
            ),
            0.0
        );
        assert!(engine.drain_jobs(&realm).unwrap() > 0);
        assert_eq!(number(&engine, &realm, "observed;"), 1.0);
        assert_eq!(engine.drain_jobs(&realm).unwrap(), 0);
    }
}

#[test]
fn checkpoint_drains_reactions_enqueued_by_reactions_in_fifo_order() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    assert_eq!(
        number(
            &engine,
            &realm,
            "async function seed(){return 0;} var order=0; \
             seed().then(function(){order=order*10+1; \
                 seed().then(function(){order=order*10+3;});}); \
             seed().then(function(){order=order*10+2;}); order;",
        ),
        0.0
    );
    assert!(engine.drain_jobs(&realm).unwrap() >= 3);
    assert_eq!(number(&engine, &realm, "order;"), 123.0);
    assert_eq!(engine.drain_jobs(&realm).unwrap(), 0);
}

#[test]
fn javascript_rejection_recovery_reaches_the_next_reaction() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    assert_eq!(
        number(
            &engine,
            &realm,
            "async function fail(){throw 7;} var recovered=0; \
             fail().then(undefined,function(reason){return reason+1;}) \
                 .then(function(value){recovered=value;}); recovered;",
        ),
        0.0
    );
    assert!(engine.drain_jobs(&realm).unwrap() >= 2);
    assert_eq!(number(&engine, &realm, "recovered;"), 8.0);
}
