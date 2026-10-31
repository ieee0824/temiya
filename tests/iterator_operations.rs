use temiya::{
    Engine,
    embedding::ScriptBackend,
    runtime::{Object, Value},
};

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
fn public_array_and_argument_consumers_keep_protocol_order_and_cached_next() {
    for consumer in [
        "let values=[...iterable];let result=values[0]+values[1];",
        "let result=sum(...iterable);",
    ] {
        check(&format!(
            "let trace='';let index=0;let iterator={{get next(){{trace+='N';return function(){{if(this!==iterator)throw 42;trace+='S';let current=index++;if(current===0)Object.defineProperty(iterator,'next',{{value:function(){{throw 42;}}}});return {{get done(){{trace+='D';return current===2;}},get value(){{trace+='V';return current===0?20:22;}}}};}};}}}};let iterable={{get [Symbol.iterator](){{trace+='I';return function(){{if(this!==iterable)throw 42;trace+='C';return iterator;}};}}}};function sum(a,b){{return a+b;}}{consumer}result===42&&trace==='ICNSDVSDVSD';"
        ));
    }
}

#[test]
fn iterator_failures_preserve_identity_and_do_not_call_return() {
    for next in [
        "get next(){throw marker;}",
        "next(){throw marker;}",
        "next(){return {get done(){throw marker;}};}",
        "next(){return {done:false,get value(){throw marker;}};}",
    ] {
        for consumer in ["[...iterable];", "sum(...iterable);"] {
            check(&format!(
                "let marker={{}};let closed=0;let iterable={{[Symbol.iterator](){{return {{{next},return:function(){{closed++;return {{}};}}}};}}}};function sum(){{throw 42;}}let caught=false;try{{{consumer}}}catch(e){{caught=e===marker;}}caught&&closed===0;"
            ));
        }
    }
    check(
        "let closed=0;let iterable={[Symbol.iterator](){return {next(){return 0;},return:function(){closed++;return {};}};}};let caught=false;try{[...iterable];}catch(e){caught=e instanceof TypeError;}caught&&closed===0;",
    );
    check("let values=[...'😀x'];values.length===2&&values[0]==='😀'&&values[1]==='x';");
}

#[test]
fn consumer_failure_closes_once_and_preserves_original_throw() {
    check(
        "let gets=0;let calls=0;let marker={};let iterable={[Symbol.iterator](){return {next(){return {done:false,value:0};},get return(){gets++;return function(){calls++;throw marker;};}};}};function collect(){}let caught=false;try{collect(...iterable);}catch(e){caught=e instanceof RangeError;}caught&&gets===1&&calls===1;",
    );
}

#[test]
fn live_iterator_and_result_edges_survive_reentrant_collection_then_release() {
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
            let source = "function make(){let index=0;return {[Symbol.iterator](){gcNow();return {next(){gcNow();let current=++index;return {get done(){gcNow();return current>2;},get value(){gcNow();return {answer:current===1?20:22};}};}};}};}let values=[...make()];function add(a,b){gcNow();return a.answer+b.answer;}gcNow();values[0].answer+values[1].answer===42&&add(...make())===42;";
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
fn builtin_iterator_results_keep_plain_object_order_and_mutable_descriptors() {
    check(
        "let iterator=[42][Symbol.iterator]();let first=iterator.next();let last=iterator.next();let value=Object.getOwnPropertyDescriptor(first,'value');let done=Object.getOwnPropertyDescriptor(first,'done');let keys=Reflect.ownKeys(first);let valid=Object.getPrototypeOf(first)===Object.prototype&&keys[0]==='value'&&keys[1]==='done'&&value.writable&&value.enumerable&&value.configurable&&done.writable&&done.enumerable&&done.configurable&&first.value===42&&!first.done&&last.value===undefined&&last.done;first.value=20;delete first.done;valid&&first.value===20&&!('done' in first);",
    );
}
