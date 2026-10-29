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

#[test]
fn shared_agent_realms_preserve_cross_realm_enqueue_order() {
    let engine = Engine::default();
    let parent = engine.create_realm();
    let child = engine.create_realm_sharing_jobs(&parent).unwrap();
    assert!(
        Engine::default()
            .create_realm_sharing_jobs(&parent)
            .is_err()
    );
    let order = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    for (realm, value) in [(&child, 1), (&parent, 2)] {
        let weak = std::rc::Rc::downgrade(&order);
        engine
            .enqueue_job(realm, move || {
                weak.upgrade().unwrap().borrow_mut().push(value);
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(engine.drain_jobs(&parent).unwrap(), 2);
    assert_eq!(*order.borrow(), [1, 2]);
    assert_eq!(engine.drain_jobs(&child).unwrap(), 0);
}

#[test]
fn shared_agent_checkpoint_allows_a_handler_attached_by_another_job() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let reports = std::rc::Rc::new(std::cell::Cell::new(0));
    let weak = std::rc::Rc::downgrade(&reports);
    engine
        .register_job_error_callback(&realm, move |_| {
            let value = weak.upgrade().unwrap();
            value.set(value.get() + 1);
        })
        .unwrap();
    number(
        &engine,
        &realm,
        "async function bad(){throw 7;}async function tick(){return 0;}var handled=0;var p=bad();tick().then(function(){p.then(undefined,function(){handled=1;});});0;",
    );
    engine.drain_jobs(&realm).unwrap();
    assert_eq!(number(&engine, &realm, "handled;"), 1.0);
    assert_eq!(reports.get(), 0);
}

#[test]
fn shared_agent_jobs_keep_cross_realm_continuations_alive_during_collection() {
    let engine = std::rc::Rc::new(Engine::default());
    let parent = engine.create_realm();
    let child = engine.create_realm_sharing_jobs(&parent).unwrap();
    let weak = std::rc::Rc::downgrade(&engine);
    engine
        .register_global_function(&parent, "collect", move |context, _, _| {
            weak.upgrade()
                .unwrap()
                .collect_garbage()
                .map_err(|e| temiya::embedding::HostError::type_error(e.to_string()))?;
            Ok(context.value(Value::Undefined))
        })
        .unwrap();
    engine
        .set_global(
            &child,
            "parent",
            engine
                .value(&child, Value::Object(parent.record().global_object()))
                .unwrap(),
        )
        .unwrap();
    number(&engine, &parent, "var order='';0;");
    number(
        &engine,
        &child,
        "async function task(){await 1;parent.collect();parent.order+=1;}task();0;",
    );
    number(
        &engine,
        &parent,
        "async function task(){await 1;collect();order+=2;}task();0;",
    );
    engine.collect_garbage().unwrap();
    engine.drain_jobs(&parent).unwrap();
    let script = engine.compile(&parent, "order==='12';").unwrap();
    assert!(matches!(
        engine.evaluate(&script).unwrap().value(),
        Value::Boolean(true)
    ));
}
