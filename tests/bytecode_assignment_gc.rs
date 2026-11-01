use temiya::{
    Engine,
    embedding::ScriptBackend,
    runtime::{Object, Value},
};

#[test]
fn assignment_vm_reentrant_gc_keeps_ephemeral_references_and_releases_them() {
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Bytecode)
        .build();
    {
        let realm = engine.create_realm();
        let gc =
            Object::native_function_with_context(engine.allocation_context(), |context, _, _| {
                context.collect_garbage()?;
                Ok(Value::Undefined)
            });
        engine
            .set_global(
                &realm,
                "gcNow",
                engine.value(&realm, Value::Object(gc)).unwrap(),
            )
            .unwrap();
        // The targets have no other JS owner while callbacks collect the heap.
        // Wrapping in a function permits shared object-method metadata without
        // falling back from whole-Script bytecode compilation.
        let source = "function run(){let writes=0;function target(){return {set value(v){gcNow();writes+=v;},set rest(v){gcNow();writes+=v.answer;}};}[target().value=(gcNow(),20)]=[undefined];({answer:target().value=(gcNow(),22)}={get answer(){gcNow();return undefined;}});({...target().rest}={get answer(){gcNow();return 42;}});[...({set values(v){gcNow();if(v[0].answer!==42)throw 1;}}).values]=[{answer:42}];return writes===84;}run();";
        let script = engine.compile(&realm, source).unwrap();
        assert_eq!(script.backend(), ScriptBackend::Bytecode);
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
    assert_eq!(engine.collect_garbage().unwrap().retained, 0);
}

#[test]
fn static_assignment_reference_survives_getter_rebinding_its_only_owner() {
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Bytecode)
        .build();
    {
        let realm = engine.create_realm();
        let gc =
            Object::native_function_with_context(engine.allocation_context(), |context, _, _| {
                context.collect_garbage()?;
                Ok(Value::Undefined)
            });
        engine
            .set_global(
                &realm,
                "gcNow",
                engine.value(&realm, Value::Object(gc)).unwrap(),
            )
            .unwrap();
        let source = "function run(){let writes=0;let obj={set value(v){gcNow();writes+=v;}};({x:obj.value}={get x(){obj=null;gcNow();return 42;}});return writes===42&&obj===null;}run();";
        let script = engine.compile(&realm, source).unwrap();
        assert_eq!(script.backend(), ScriptBackend::Bytecode);
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
    assert_eq!(engine.collect_garbage().unwrap().retained, 0);
}

#[test]
fn nested_static_assignment_keeps_parent_value_and_child_reference_rooted() {
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Bytecode)
        .build();
    {
        let realm = engine.create_realm();
        let gc =
            Object::native_function_with_context(engine.allocation_context(), |context, _, _| {
                context.collect_garbage()?;
                Ok(Value::Undefined)
            });
        engine
            .set_global(
                &realm,
                "gcNow",
                engine.value(&realm, Value::Object(gc)).unwrap(),
            )
            .unwrap();
        let source = "function run(){let writes=0;let obj={set value(v){gcNow();writes+=v;}};({outer:{x:obj.value}}={get outer(){gcNow();return {get x(){obj=null;gcNow();return 42;}};}});return writes===42&&obj===null;}run();";
        let script = engine.compile(&realm, source).unwrap();
        assert_eq!(script.backend(), ScriptBackend::Bytecode);
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
    assert_eq!(engine.collect_garbage().unwrap().retained, 0);
}
