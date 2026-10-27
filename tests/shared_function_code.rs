use temiya::{
    Engine,
    ast::StmtKind,
    lexer::Span,
    parser::parse_script,
    runtime::{Completion, Realm, ScriptRecord, Value},
};

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
fn repeated_function_code_keeps_independent_closures_and_own_prototypes() {
    for source in [
        "function make(x){return function(y){return x+y;};} var a=make(1),b=make(2); a!==b && a(41)===42 && b(40)===42 && a.prototype!==b.prototype;",
        "function make(x){return function named(n){if(n===0)return x;return named(n-1);};} var a=make(41),b=make(42); a(2)===41 && b(2)===42;",
        "function make(base, f=()=>base){return f;} var a=make(41),b=make(42); a()===41 && b()===42;",
        "function make(x){return {value:20,make(){return ()=>this.value+x;}};} var a=make(21).make(),b=make(22).make(); a()===41 && b()===42;",
    ] {
        assert!(run(source).same_value(&Value::Boolean(true)), "{source}");
    }
}

#[test]
fn functions_survive_their_compiled_script_and_stay_separate_across_scripts() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    {
        let script = engine
            .compile(&realm, "var saved=(function(x){return ()=>x;})(41);")
            .unwrap();
        engine.evaluate(&script).unwrap();
    }
    let script = engine
        .compile(
            &realm,
            "var other=(function(x){return ()=>x;})(42); saved()===41 && other()===42;",
        )
        .unwrap();
    assert!(
        engine
            .evaluate(&script)
            .unwrap()
            .value()
            .same_value(&Value::Boolean(true))
    );
}

#[test]
fn generator_continuations_retain_their_own_function_captures() {
    assert!(run("function* make(x){yield ()=>x; yield ()=>x+1;} var a=make(40),b=make(41); var a0=a.next().value,b0=b.next().value,a1=a.next().value,b1=b.next().value; a0()===40 && b0()===41 && a1()===41 && b1()===42;").same_value(&Value::Boolean(true)));
}

#[test]
fn public_owned_ast_span_collisions_do_not_reuse_different_code() {
    let mut program =
        parse_script("function a(){return 11;} function b(){return 31;} a()+b();").unwrap();
    for statement in &mut program.body {
        if let StmtKind::FunctionDeclaration(function) = &mut statement.kind {
            function.span = Span { start: 0, end: 0 };
        }
    }
    let script = ScriptRecord::new(Realm::ordinary(), program);
    let Completion::Normal(Some(value)) = script.evaluate().unwrap() else {
        panic!()
    };
    assert!(value.same_value(&Value::Number(42.0)));
}

#[test]
fn discarded_generator_bodies_do_not_leave_reusable_definition_addresses() {
    assert!(run("function* first(x){yield function(){return x+1;};} function* second(x){yield function(){return x+2;};} var ok=true; for(var i=0;i<100;i++){var a=first(40).next().value; var b=second(40).next().value; if(a()!==41 || b()!==42)ok=false;} ok;").same_value(&Value::Boolean(true)));
}

#[test]
fn reevaluating_one_script_keeps_each_returned_closure_independent() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    engine
        .evaluate(&engine.compile(&realm, "var next=40;").unwrap())
        .unwrap();
    let script = engine
        .compile(&realm, "(function(x){return ()=>x;})(next++);")
        .unwrap();
    let first = engine.evaluate(&script).unwrap();
    let second = engine.evaluate(&script).unwrap();
    drop(script);
    for (handle, expected) in [(first, 40.0), (second, 41.0)] {
        let Value::Object(function) = handle.value() else {
            panic!()
        };
        assert!(
            function
                .call(Value::Undefined, &[])
                .unwrap()
                .same_value(&Value::Number(expected))
        );
    }
}
