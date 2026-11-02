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
fn value_calls_keep_receiver_proxy_errors_and_legacy_properties() {
    check(
        "function f(call){var unused=0;return call();}let receiver=42,count=-1;let proxy=new Proxy(function(){return 42;},{apply(target,thisArg,args){receiver=thisArg;count=args.length;return 20;}});f(function(){'use strict';return this;})===undefined&&f(proxy)===20&&receiver===undefined&&count===0&&f(()=>f.arguments===null&&f.caller===null);",
    );
    check(
        "function f(call){return call();}function twice(call){return call()();}let token={},caught=false,bad=false;try{f(()=>{throw token;});}catch(e){caught=e===token;}try{f(42);}catch(e){bad=e instanceof TypeError;}caught&&bad&&twice(()=>()=>20)===20;",
    );
    check(
        "let hits=0;class Base{get read(){hits++;return function(){'use strict';return this;};}}class Derived extends Base{readValue(){return (0,super.read)();}}new Derived().readValue()===undefined&&hits===1;",
    );
}

#[test]
fn calls_keep_saved_references_and_validate_fallback_before_effects() {
    check(
        "let old={result:0},target=old,replacement={result:20};function f(call){target.result=call();return target.result;}f(()=>{target=replacement;return 42;})===20&&old.result===42&&replacement.result===20;",
    );
    check(
        "let hits=0;function f(call){call();return ()=>42;}let read=f(()=>{hits++;});hits===1&&read()===42;",
    );
    check(
        "let hits=0;function f(call){if(call())throw 20;return 42;}let caught=false;try{f(()=>{hits++;return true;});}catch(e){caught=e===20;}caught&&hits===1&&f(()=>{hits++;return false;})===42&&hits===2;",
    );
}

#[test]
fn callbacks_keep_local_objects_and_returned_values_alive_during_gc() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        {
            let realm = engine.create_realm();
            let gc = temiya::runtime::Object::native_function_with_context(
                engine.allocation_context(),
                |context, _, _| {
                    context.collect_garbage()?;
                    Ok(Value::Number(20.0))
                },
            );
            engine
                .set_global(
                    &realm,
                    "gcNow",
                    engine.value(&realm, Value::Object(gc)).unwrap(),
                )
                .unwrap();
            let script=engine.compile(&realm,"function f(call,obj){let result=call();return obj.answer+result;}function g(call){let result=call();return result.answer;}f(gcNow,{answer:22})===42&&g(()=>{gcNow();return {answer:20};})===20;").unwrap();
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
