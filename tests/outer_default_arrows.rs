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
fn outer_default_arrows_keep_fresh_names_live_bindings_and_dynamic_lookup() {
    check(
        "let outer=1;function f(read=()=>outer){var outer=20;return read;}let a=f();outer=42;let b=f(undefined);let supplied=()=>7;let make=function named(read=()=>named){var named=0;return read;};a!==b&&a.name==='read'&&a.length===0&&f.length===0&&a()===42&&b()===42&&f(supplied)===supplied&&f(null)===null&&make()()===make;",
    );
    check(
        "let outer=42,gets=0,holder={get outer(){gets++;return 99;}};let make;with(holder){make=function(read=()=>outer){var outer=0;return read;};}let read=make();let untouched=gets===0;untouched&&read()===99&&gets===1&&read()===99&&gets===2;",
    );
    check(
        "function f(read=()=>pending){var pending=0;return read;}let read=f(),early=false;try{read();}catch(e){early=e instanceof ReferenceError;}let pending=42;early&&read()===42;",
    );
}

#[test]
fn parameter_and_lexical_context_arrows_keep_the_full_header_path() {
    check(
        "function local(a=1,read=()=>a){var a=42;return read;}function later(read=()=>value,value=20){return read();}function Item(read=()=>new.target){this.read=read;}function args(value,read=()=>arguments){return read()[0];}local()()===1&&local(20)()===20&&later()===20&&new Item().read()===Item&&args(7)===7;",
    );
    check(
        "let outer=41,hits=0;function mixed(read=()=>outer,b=++hits){var outer=0;return read()+b;}mixed()===42&&hits===1&&mixed(undefined,20)===61&&hits===1&&mixed(undefined,undefined)===43&&hits===2;",
    );
}
