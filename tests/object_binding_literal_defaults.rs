use temiya::{
    Engine,
    embedding::ScriptBackend,
    runtime::{AllocationContext, Object, Value},
};

#[test]
fn literal_projection_preserves_order_defaults_names_and_fallback_receivers() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        for source in [
            "function f({é:a,'\\u{10400}':b,'\\ud800':c}={'é':1,𐐀:20,'\\ud800':21}){return a+b+c;}f()===42;",
            "function f({x}={x:1,'x':42}){return x;}f()===42;",
            "let trace='';function f({a,b}={a:(trace+='a',1),a:(trace+='A',20),b:(trace+='b',22),unused:(trace+='u',0)}){return a+b;}f()===42&&trace==='aAbu';",
            "function f({fn}={fn:function(){}}){return fn.name;}function g({fn}=({fn:function(){}})){return fn.name;}f()===g();",
            "function f({C}={C:class{}}){return C.name;}function g({C}=({C:class{}})){return C.name;}f()===g();",
            "function f({fn=()=>42}={fn:undefined}){return fn.name==='fn'&&fn()===42;}f();",
            "let trace='';Object.defineProperty(Object.prototype,'missing',{get(){trace+='g';return this.marker;},configurable:true});function f({missing}={marker:42}){return missing;}f()===42&&trace==='g';",
            "Object.defineProperty(Object.prototype,'x',{get(){throw 1;},configurable:true});function f({x=42}={x:undefined}){return x;}f()===42;",
            "let trace='';function f({x}={get x(){trace+='g';return 42;}}){return x;}f()===42&&trace==='g';",
            "let trace='';let source={get x(){trace+='g';return 42;}};function f({x}={...source}){return x;}f()===42&&trace==='g';",
            "function f({x}={__proto__:{x:42}}){return x;}f()===42;",
        ] {
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
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn unused_foreign_property_fails_before_later_effects_and_binding_initialization() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let foreign_context = AllocationContext::new();
        let foreign = Object::new_in(&foreign_context);
        let callback = Object::native_function_in(engine.allocation_context(), move |_, _| {
            Ok(Value::Object(foreign.clone()))
        });
        engine
            .set_global(
                &realm,
                "foreign",
                engine.value(&realm, Value::Object(callback)).unwrap(),
            )
            .unwrap();
        let script=engine.compile(&realm,"let marker=0;let saved;function read({answer}={answer:42,capture:(saved=()=>answer),unused:foreign(),later:(marker=1)}){return false;}function run(){try{read();return false;}catch(e){if(!(e instanceof TypeError)||marker!==0)return false;}try{saved();return false;}catch(e){return e instanceof ReferenceError;}}run();").unwrap();
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
fn projected_object_values_remain_rooted_through_later_property_and_default_gc() {
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Auto)
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
        let script=engine.compile(&realm,"function read({a,b={answer:22}}={a:(gcNow(),{answer:20}),b:undefined,unused:gcNow()}){gcNow();return a.answer+b.answer;}read()===42;").unwrap();
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
    assert_eq!(engine.collect_garbage().unwrap().retained, 0);
}

#[test]
fn projection_keeps_literal_effects_before_iterator_close_and_original_throw() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script=engine.compile(&realm,"let trace='';let marker={};let iterable={[Symbol.iterator](){trace+='i';return{next(){trace+='n';return{done:false,value:undefined};},return(){trace+='r';return{done:true};}};}};function fail(){trace+='d';throw marker;}function read({values:[x=fail()]}={values:iterable,unused:(trace+='p',0)}){return false;}function run(){try{read();return false;}catch(e){return e===marker&&trace==='pindr';}}run();").unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}"
        );
    }
}
