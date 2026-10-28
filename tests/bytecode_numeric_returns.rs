use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert_eq!(script.backend(), backend);
        assert!(
            engine
                .evaluate(&script)
                .unwrap()
                .value()
                .same_value(&Value::Boolean(true)),
            "{backend:?}: {source}"
        );
    }
}

#[test]
fn numeric_returns_preserve_ieee_bits_comparisons_and_parameter_binding() {
    for source in [
        "function f(x){return -x;} 1/f(0)===-Infinity && 1/f(-0)===Infinity;",
        "function f(x,y){return x%y;} 1/f(-4,2)===-Infinity && f(5,2)===1;",
        "function f(x){return ~x;} f(Infinity)===-1 && f(4294967296)===-1;",
        "function f(x,y){return x<y;} f(1,2) && !f(NaN,2);",
        "function f(x,x){return x+1;} let missing=f(3); f(3,41,{})===42 && missing!==missing;",
        "function f(x){return x+x;} f(21n)===42n && f('a')==='aa';",
        "function f(x){'use strict';return x+1;} f.call(7,41)===42;",
        "let f=(x)=>x+1; f(41)===42;",
        "function outer(arguments){return function(){return arguments;};} typeof outer(40)()==='object';",
        "function outer(arguments){return ()=>arguments+2;} outer(40)()===42;",
    ] {
        check(source);
    }
}

#[test]
fn captured_number_guards_observe_updates_and_fall_back_without_coercion() {
    for source in [
        "function make(n){return {read:(x)=>x+n,set:(v)=>n=v};} let pair=make(2); let ok=pair.read(3)===5; pair.set(7); ok=ok&&pair.read(3)===10; pair.set('!'); ok=ok&&pair.read(3)==='3!'; pair.set(2); ok&&pair.read(3)===5;",
        "let log=''; function f(x){return x+2;} let input={valueOf:function(){log+='V';return 40;}}; f(input)===42 && log==='V';",
        "let log=''; let input={valueOf:function(){log+='V';return 40;}}; let f=(function(n){return function(x){return x+n;};})(input); f(2)===42 && log==='V';",
        "let log=''; Object.defineProperty(this,'n',{get:function(){log+='G';return 40;}}); function f(x){return x+n;} f(2)===42 && log==='G';",
        "function outer(n){return function(x){return x+n;var n;};} let result=outer(40)(2); result!==result;",
        "function f(x){return (x===1)+1;} f(1)===2 && f(2)===1;",
    ] {
        check(source);
    }
}

#[test]
fn capture_tdz_remains_a_source_realm_reference_error() {
    use temiya::embedding::ExceptionKind;
    for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine
            .compile(&realm, "{let f=(x)=>x+n;f(1);let n=2;}")
            .unwrap();
        assert_eq!(
            engine.evaluate(&script).unwrap_err().kind(),
            ExceptionKind::ReferenceError
        );
    }
}
