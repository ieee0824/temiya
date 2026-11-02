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
fn prepared_stores_and_calls_keep_live_independent_parameter_and_body_cells() {
    check(
        "function f(x=1,read=()=>x){var x=42;return read();}function g(x=1,write=()=>{x=20;},read=()=>x){var local=0;write();return read();}f()===1&&g()===20;",
    );
    check(
        "let key='answer';function f({[key]:value}){var key='missing';return value;}let outer=42;function g({value,read=()=>outer}){var outer=0;if(value!==read())throw value;return value;}f({answer:20})===20&&g({value:42})===42;",
    );
    check(
        "let hits=0;function f({flag,read}){if(flag)return read();return 42;}f({flag:false,read(){hits++;return 20;}})===42&&hits===0&&f({flag:true,read(){hits++;return 20;}})===20&&hits===1;",
    );
}

#[test]
fn prepared_calls_keep_receiver_errors_proxy_apply_and_fallback_order() {
    check(
        "function f({read}){return read();}let receiver='unset',count=-1;let p=new Proxy(function(){return 42;},{apply(target,thisArg,args){receiver=thisArg;count=args.length;return 20;}});f({read:function(){'use strict';return this;}})===undefined&&f({read:p})===20&&receiver===undefined&&count===0;",
    );
    check(
        "function f({read}){return read();}let thrown={},caught=false,bad=false;try{f({read(){throw thrown;}});}catch(e){caught=e===thrown;}try{f({read:42});}catch(e){bad=e instanceof TypeError;}caught&&bad;",
    );
    check(
        "let hits=0;function f({x,read}){read();return ()=>x;}let result=f({x:42,read(){hits++;}});hits===1&&result()===42;",
    );
}

#[test]
fn prepared_call_keeps_default_closure_cells_rooted_during_gc() {
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
            let script = engine.compile(&realm,"function f(value={answer:42},read=()=>{gcNow();return value;}){var value=20;return read();}f().answer===42;").unwrap();
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
