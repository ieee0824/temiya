use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}: {source}"
        );
    }
}

#[test]
fn lexical_reads_observe_live_cells_tdz_and_missing_names() {
    check(
        "function make(read=()=>later,later=read()){return later;}let caught=0;try{make();}catch(error){if(error instanceof ReferenceError)caught++;}function missing(read=()=>absentName){return read;}try{missing()();}catch(error){if(error instanceof ReferenceError)caught++;}caught===2;",
    );
    check(
        "let value=20;function make(read=()=>value){var value=99;return read;}let read=make();let effects=0;let first=read(++effects);value=42;first===20&&read()===42&&effects===1;",
    );
    check(
        "let read=()=>value;let caught=0;try{read();}catch(error){if(error instanceof ReferenceError)caught++;}let value=42;let missing=()=>absentName;try{missing();}catch(error){if(error instanceof ReferenceError)caught++;}caught===2&&read()===42;",
    );
}

#[test]
fn lexical_reads_preserve_with_getters_and_outer_arguments() {
    check(
        "let hits=0;let object={get answer(){hits++;return hits;}};function make(){with(object){return ()=>answer;}}let read=make();read()===1&&read()===2&&hits===2;",
    );
    check(
        "function make(value,read=()=>arguments){var arguments={0:99};return read;}let read=make(42);read()[0]===42;",
    );
}

#[test]
fn captured_parameter_objects_survive_gc_after_body_reassignment() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        {
            let realm = engine.create_realm();
            let gc = temiya::runtime::Object::native_function_with_context(
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
            let script = engine.compile(&realm, "function make(value={answer:42},read=()=>value){var value={answer:99};return read;}let saved=make();gcNow();saved().answer===42;").unwrap();
            assert!(
                matches!(
                    engine.evaluate(&script).unwrap().value(),
                    Value::Boolean(true)
                ),
                "{backend:?}"
            );
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}
