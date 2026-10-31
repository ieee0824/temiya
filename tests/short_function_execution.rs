use temiya::{
    Engine,
    embedding::ScriptBackend,
    runtime::{Object, Value},
};

#[test]
fn metadata_observer_elision_keeps_lexical_capture_and_observer_fallbacks() {
    let source = "let counter=0;let key={toString(){counter++;return 'answer';}};function make(){return {[key]:42,get captured(){return counter;},method(){return 22;}};}let first=make();let second=make();function receiver(){return {read:()=>this.answer};}function target(){return {value:new.target};}function evaluated(){return {answer:arguments[0]};}let owner={answer:42,receiver,evaluated};first.answer===42&&second.answer===42&&first.captured===2&&first.method()===22&&owner.receiver().read()===42&&owner.evaluated(42).answer===42&&new target().value===target;";
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}"
        );
    }
}

#[test]
fn short_callbacks_preserve_effects_abrupt_values_and_fallbacks() {
    let source = "let state=0,reads=0,current=20,n=19;function done(){if(state!==0)throw 42;state=1;reads++;return current>n;}let result=done();let caught=false;try{done();}catch(e){caught=e===42;}let a=0,b=0;function nested(){a=(b=22);return a;}let marker={};function fail(){throw marker;}let identity=false;try{fail();}catch(e){identity=e===marker;}result&&caught&&reads===1&&state===1&&nested()===22&&b===22&&identity;";
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}"
        );
    }
}

#[test]
fn metadata_factories_and_coercion_callbacks_keep_roots_during_collection() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
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
            let source = "let left={valueOf(){gcNow();return 20;}};let right={valueOf(){gcNow();return 22;}};function sum(){return left+right;}function factory(){return {get answer(){return sum();},nested(){return left;}};}let object=factory();gcNow();object.answer===42&&object.nested()===left;";
            let script = engine.compile(&realm, source).unwrap();
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

#[test]
fn object_environment_callbacks_keep_assignment_resolution_order() {
    let source = "let trace='';let object=new Proxy({x:0},{has(target,key){if(key==='x'){trace+='H';return true;}return false;},set(target,key,value,receiver){trace+='S';return Reflect.set(target,key,value,receiver);}});let value={valueOf(){trace+='V';return 42;}};let run;with(object){run=function(){x=+value;return 0;};}run();object.x===42&&trace==='HVS';";
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}"
        );
    }
}

#[test]
fn short_lookup_failures_keep_the_defining_realm_and_environment() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let foreign = engine.create_realm();
        let local = engine.create_realm();
        for (name, source) in [
            (
                "fail",
                "let calls=0;function fail(){calls++;return absent;}fail;",
            ),
            ("ForeignReferenceError", "ReferenceError;"),
        ] {
            let script = engine.compile(&foreign, source).unwrap();
            let value = engine.evaluate(&script).unwrap();
            engine
                .set_global(
                    &local,
                    name,
                    engine.value(&local, value.value().clone()).unwrap(),
                )
                .unwrap();
        }
        let script=engine.compile(&local,"let absent=42;let caught=false;try{fail();}catch(e){caught=e instanceof ForeignReferenceError&&!(e instanceof ReferenceError);}caught;").unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}"
        );
    }
}
