use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    runtime::{Completion, Object, Realm, RuntimeErrorKind, Value},
};

fn value(realm: &Realm, source: &str) -> Value {
    let code =
        BytecodeCompiler::compile(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    match VirtualMachine::execute_in_realm(&code, realm).unwrap() {
        Completion::Normal(Some(value)) => value,
        completion => panic!("{completion:?}"),
    }
}

#[test]
fn function_and_arrow_bodies_execute_verified_bytecode_with_parameters() {
    for source in [
        "(function(a,b){return a+b;})(20,22);",
        "((a,b)=>a+b)(20,22);",
        "((a,b)=>{return a+b;})(20,22);",
        "(function(a){var a;return a;})(42);",
        "(function(a,a){return a;})(1,42);",
        "(function(){return arguments[0]+arguments[1];})(20,22);",
    ] {
        assert!(
            value(&Realm::ordinary(), source).same_value(&Value::Number(42.0)),
            "{source}"
        );
    }
}

#[test]
fn escaped_captures_share_mutations_and_outlive_code_and_realm_handles() {
    let closure = {
        let realm = Realm::ordinary();
        value(
            &realm,
            "(function(start){let value=start;return function(step){value=value+step;return value;};})(20);",
        )
    };
    let Value::Object(function) = closure else {
        panic!("not a function")
    };
    assert!(
        function
            .call(Value::Undefined, &[Value::Number(1.0)])
            .unwrap()
            .same_value(&Value::Number(21.0))
    );
    assert!(
        function
            .call(Value::Undefined, &[Value::Number(21.0)])
            .unwrap()
            .same_value(&Value::Number(42.0))
    );
}

#[test]
fn arrows_capture_lexical_this_arguments_and_new_target() {
    for (source, expected) in [
        (
            "this.answer=42;var maker=function(){return ()=>this.answer;};var get=maker.call(this);get();",
            Value::Number(42.0),
        ),
        (
            "(function(){return ()=>arguments[0];})(42)();",
            Value::Number(42.0),
        ),
        (
            "var C=function(){this.get=()=>new.target;};var instance=new C();instance.get()===C;",
            Value::Boolean(true),
        ),
        (
            "(function(){'use strict';return this;})();",
            Value::Undefined,
        ),
        (
            "(function(){'use strict';return this;}).call(42);",
            Value::Number(42.0),
        ),
    ] {
        assert!(
            value(&Realm::ordinary(), source).same_value(&expected),
            "{source}"
        );
    }
    let global = Object::new();
    let this = Object::new();
    this.create_data_property("answer".into(), Value::Number(42.0))
        .unwrap();
    let realm = Realm::new(global, Value::Object(this));
    assert!(value(&realm, "(function(){return this.answer;})();").same_value(&Value::Number(42.0)));
    assert!(value(&realm, "(()=>this.answer)();").same_value(&Value::Number(42.0)));
}

#[test]
fn named_recursion_and_per_iteration_block_captures_are_real_environments() {
    assert!(
        value(
            &Realm::ordinary(),
            "(function self(n){if(n===0){return 42;}return self(n-1);})(10);"
        )
        .same_value(&Value::Number(42.0))
    );
    let source = "let i=0;let run=true;let first;let second;while(run){let snapshot=i;if(i===0){first=()=>snapshot;}else{second=()=>snapshot;run=false;}i=i+1;}first()*100+second();";
    assert!(value(&Realm::ordinary(), source).same_value(&Value::Number(1.0)));
    assert!(
        value(
            &Realm::ordinary(),
            "(function self(){self=0;return self.name;})();"
        )
        .same_value(&Value::String("self".into()))
    );
}

#[test]
fn captured_tdz_const_throw_and_recursion_failure_preserve_values() {
    for (source, kind) in [
        (
            "var get=()=>later;get();let later=42;",
            RuntimeErrorKind::ReferenceError,
        ),
        (
            "const answer=42;var change=()=>{answer=0;};change();",
            RuntimeErrorKind::TypeError,
        ),
        ("new (()=>42)();", RuntimeErrorKind::TypeError),
        (
            "(function recur(){return recur();})();",
            RuntimeErrorKind::RangeError,
        ),
    ] {
        let realm = Realm::ordinary();
        let code = BytecodeCompiler::compile(source).unwrap();
        assert_eq!(
            VirtualMachine::execute_in_realm(&code, &realm)
                .unwrap_err()
                .kind(),
            kind,
            "{source}"
        );
        assert!(value(&realm, "(()=>42)();").same_value(&Value::Number(42.0)));
    }
    let realm = Realm::ordinary();
    let code = BytecodeCompiler::compile("(()=>{throw 17;})();").unwrap();
    let error = VirtualMachine::execute_in_realm(&code, &realm).unwrap_err();
    assert!(
        error
            .thrown_value()
            .unwrap()
            .same_value(&Value::Number(17.0))
    );
}

#[test]
fn unfinished_function_forms_fail_compilation_instead_of_ast_fallback() {
    for source in [
        "(function(a=42){return a;})();",
        "((...args)=>args[0])(42);",
        "(function({a}){return a;})({a:42});",
        "(async function(){return 42;})();",
        "(function*(){yield 42;})();",
        "function f(){return 42;} f();",
        "(function(a){let a=42;return a;})(1);",
    ] {
        assert!(BytecodeCompiler::compile(source).is_err(), "{source}");
    }
}

#[test]
fn sloppy_primitive_this_uses_actual_wrapper_slots_and_prototypes() {
    for (source, expected) in [
        (
            "(function(){return this.valueOf();}).call(42);",
            Value::Number(42.0),
        ),
        (
            "(function(){return this.length;}).call('𝄞');",
            Value::Number(2.0),
        ),
    ] {
        assert!(
            value(&Realm::ordinary(), source).same_value(&expected),
            "{source}"
        );
    }
    let realm = Realm::ordinary();
    let Value::Object(boxed) = value(&realm, "(function(){return this;}).call(42n);") else {
        panic!("BigInt this was not boxed")
    };
    assert_eq!(
        boxed.prototype(),
        Some(realm.intrinsic(temiya::runtime::IntrinsicId::BigIntPrototype))
    );
}
