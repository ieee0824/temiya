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
fn guards_read_getters_once_and_only_execute_the_selected_tail() {
    check(
        "let order='';let obj={get flag(){order+='a';return false;},get thrown(){order+='x';return 7;},get result(){order+='b';return 42;}};function f(x){if(x.flag)throw x.thrown;return x.result;}f(obj)===42&&order==='ab';",
    );
    check(
        "let order='',hits=0;let truth={valueOf(){hits++;return 0;}};let obj={get flag(){order+='a';return truth;},get thrown(){order+='t';throw 19;},get result(){order+='x';return 42;}};function f(x){if(x.flag)throw x.thrown;return x.result;}let caught=false;try{f(obj);}catch(e){caught=e===19;}caught&&order==='at'&&hits===0;",
    );
    check(
        "let obj={flag:true};function f(x){if(x.flag)throw x;return 42;}let caught;try{f(obj);}catch(e){caught=e;}caught===obj;",
    );
}
#[test]
fn literal_indices_preserve_proxy_keys_errors_and_dynamic_fallback() {
    check(
        "let keys='';let proxy=new Proxy({0:20,1:42},{get(target,key){keys+=key;return target[key];}});function f(x){if(x[0]!==20)throw 7;return x[1];}f(proxy)===42&&keys==='01';",
    );
    check(
        "function f(x){if(x[0])throw 7;return 42;}let caught=false;try{f(null);}catch(e){caught=e instanceof TypeError;}caught;",
    );
    check(
        "let hits=0,key={toString(){hits++;return 'flag';}};function f(x,k){if(x[k])throw 7;return 42;}f({flag:false},key)===42&&hits===1;",
    );
}
#[test]
fn writes_calls_lexicals_and_duplicate_parameters_keep_their_semantics() {
    check("function f(x){var y=x;if(y)throw 7;return y;}f(0)===0;");
    check(
        "let hits=0;function f(x){if(x.flag())throw 7;return 42;}f({flag(){hits++;return false;}})===42&&hits===1;",
    );
    check("function f(a,a){if(a!==undefined)throw 7;return a;}f(20)===undefined;");
    check(
        "function f(x){if(x){let y=42;throw y;}return 20;}let caught;try{f(true);}catch(e){caught=e;}caught===42&&f(false)===20;",
    );
}
#[test]
fn guarded_callbacks_keep_receiver_and_arguments_alive_across_gc() {
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
            let script = engine.compile(&realm, "let object={answer:42,get flag(){gcNow();this.answer=20;return false;},get result(){gcNow();return this.answer;}};function f(){if(this.flag)throw this;return this.result;}f.call(object)===20;").unwrap();
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
