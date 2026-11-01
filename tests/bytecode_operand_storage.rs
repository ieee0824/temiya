use temiya::{
    Engine,
    embedding::ScriptBackend,
    runtime::{Object, Value},
};

#[test]
fn small_and_deep_vm_operands_survive_reentrant_collection() {
    for backend in [ScriptBackend::Bytecode, ScriptBackend::Auto] {
        for source in [
            "function value(){return {valueOf(){gcNow();return 20;}};}value()+(gcNow(),22);",
            "function value(){return {valueOf(){gcNow();return 20;}};}value()+(1+(2+(3+(4+(gcNow(),12)))));",
            "function value(){return {valueOf(){throw 99;}};}value()!==(gcNow(),value())?42:0;",
        ] {
            let engine = Engine::builder().script_backend(backend).build();
            {
                let realm = engine.create_realm();
                let gc = Object::native_function_with_context(
                    engine.allocation_context(),
                    |context, _, _| {
                        context.collect_garbage()?;
                        Ok(Value::Undefined)
                    },
                );
                engine
                    .set_global(
                        &realm,
                        "gcNow",
                        engine.value(&realm, Value::Object(gc)).unwrap(),
                    )
                    .unwrap();
                let script = engine.compile(&realm, source).unwrap();
                assert!(
                    matches!(engine.evaluate(&script).unwrap().value(),Value::Number(value) if *value==42.0),
                    "{backend:?}"
                );
            }
            assert_eq!(engine.collect_garbage().unwrap().retained, 0);
        }
    }
}
