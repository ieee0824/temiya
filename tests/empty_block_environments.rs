use temiya::{Engine, runtime::Value};

fn run(source: &str) -> Value {
    let engine = Engine::default();
    let realm = engine.create_realm();
    engine
        .evaluate(&engine.compile(&realm, source).unwrap())
        .unwrap()
        .value()
        .clone()
}

#[test]
fn var_only_blocks_preserve_completion_and_loop_control() {
    for (source, expected) in [
        ("{{var value=40; value+2;}}", 42.0),
        ("for(var i=0;i<3;i++){if(i===1)continue; i+40;}", 42.0),
        (
            "var sum=0; outer: for(var i=0;i<4;i++){{if(i===2)break outer; sum+=21;}} sum;",
            42.0,
        ),
        (
            "function f(){var result=0; for(var i=0;i<2;i++){{var value=21;result+=value;}} return result;} f();",
            42.0,
        ),
    ] {
        assert!(run(source).same_value(&Value::Number(expected)), "{source}");
    }
}

#[test]
fn lexical_tdz_shadowing_and_block_function_capture_are_retained() {
    for source in [
        "let x=1; let seen=0; {let x=42; {{seen=x;}}} seen===42 && x===1;",
        "let x=42; let caught=false; {try{{x;}}catch(e){caught=e instanceof ReferenceError;} let x=1;} caught && x===42;",
        "'use strict'; var saved; {let x=42; function inner(){return x;} {{saved=inner;}}} typeof inner==='undefined' && saved()===42;",
    ] {
        assert!(run(source).same_value(&Value::Boolean(true)), "{source}");
    }
}

#[test]
fn closures_keep_var_and_block_lexical_bindings() {
    for source in [
        "var functions=[];for(var i=0;i<3;i++){{functions.push(()=>i);}} functions[0]()===3 && functions[2]()===3;",
        "var functions=[];for(var i=0;i<3;i++){let value=i;{functions.push(()=>value);}} functions[0]()===0 && functions[2]()===2;",
        "function make(){var x=41; {{return ()=>++x;}}} var f=make(); f()===42 && f()===43;",
    ] {
        assert!(run(source).same_value(&Value::Boolean(true)), "{source}");
    }
}

#[test]
fn empty_blocks_do_not_change_with_proxy_lookup_or_capture() {
    let setup = "var trace=''; var result; var saved; var p=new Proxy({x:42},{has(t,k){if(k==='x')trace+='H';return Reflect.has(t,k);},get(t,k,r){if(k==='x')trace+='G';return Reflect.get(t,k,r);}});";
    for (plain, blocked) in [
        ("with(p)result=x;", "with(p){{{result=x;}}}"),
        (
            "with(p)saved=function(){return x;};result=saved();",
            "with(p){{saved=function(){return x;};}} result=saved();",
        ),
    ] {
        let expected = run(&format!("{setup}{plain} result+':'+trace;"));
        let actual = run(&format!("{setup}{blocked} result+':'+trace;"));
        assert!(actual.same_value(&expected));
        assert!(
            actual
                .to_js_string()
                .unwrap()
                .to_string_lossy()
                .starts_with("42:")
        );
    }
}
