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
fn declarations_parameters_defaults_elisions_nested_and_rest() {
    check(
        "let [a,[b=22],...rest]=[20,[],1,2];function sum([x,y=x],z=y){return x+y+z;}let skipped=0;let input={[Symbol.iterator](){let n=0;return {next(){n++;return {done:n>2,get value(){if(n===1)throw 42;return 7;}};}};}};let [,v]=input; a+b===42&&rest.length===2&&Array.isArray(rest)&&rest[1]===2&&sum([14])===42&&v===7;",
    );
    check("var [a,b=22]=[20];a+b===42;");
}

#[test]
fn tdz_and_abrupt_close_preserve_original_throw() {
    check(
        "let closes=0;let input={[Symbol.iterator](){return {next(){return {done:false,value:undefined};},return(){closes++;throw 99;}};}};let caught=false;try{let [a=a]=input;}catch(e){caught=e instanceof ReferenceError;}caught&&closes===1;",
    );
    check(
        "let caught=false;function f([a=b],b=42){return a;}try{f([]);}catch(e){caught=e instanceof ReferenceError;}caught;",
    );
}

#[test]
fn exhausted_iterator_is_not_advanced_and_empty_pattern_closes() {
    check(
        "let calls=0;let closes=0;let input={[Symbol.iterator](){return {next(){calls++;return {done:true,get value(){throw 42;}};},return(){closes++;return {};}};}};let [a=20,,b=22,...rest]=input;let []=input;a+b===42&&calls===1&&closes===1&&rest.length===0;",
    );
}
