use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, RuntimeErrorKind, StatementEvaluator, Value},
};

fn setup(source: &str) -> Realm {
    let realm = Realm::ordinary();
    StatementEvaluator::new(realm.clone())
        .evaluate_program(&parse_script(source).unwrap())
        .unwrap();
    realm
}

fn vm(realm: &Realm, source: &str) -> Result<Completion, temiya::runtime::RuntimeError> {
    let code = BytecodeCompiler::compile(source).unwrap();
    VirtualMachine::execute_in_realm(&code, realm)
}

fn value(realm: &Realm, source: &str) -> Value {
    match vm(realm, source).unwrap() {
        Completion::Normal(Some(value)) => value,
        completion => panic!("{completion:?}"),
    }
}

#[test]
fn inline_and_heap_argument_lists_preserve_nested_calls_receivers_and_constructors() {
    let realm = setup(
        "function count(){return arguments.length;}function f(a,b,c){return a+b+c;}function C(a,b,c){this.answer=a+b+c;}var o={x:40,m:function(a,b){return this.x+a+b;}};",
    );
    for source in [
        "count()===0 && count(42)===1 && count(20,22)===2 && count(1,2,3)===3 && count(1,2,3,4,5)===5;",
        "f(20,count(1,2),20)===42;",
        "f(20,...[2,20])===42 && f(...[20],2,20)===42;",
        "o.m(1,1)===42 && new C(20,2,20).answer===42;",
    ] {
        assert!(
            value(&realm, source).same_value(&Value::Boolean(true)),
            "{source}"
        );
    }
    assert!(
        value(&realm, "function g(a,b,c){return a+b+c;}g(20,2,20);")
            .same_value(&Value::Number(42.0))
    );
}

#[test]
fn spread_arrays_strings_holes_and_constructor_arguments_use_actual_iterators() {
    let bootstrap = "var values=[20,22]; var holes=[,42]; function sum(a,b){return a+b;} function Ctor(a,b){this.answer=a+b;} function shape(){return arguments.length+':'+arguments[0].length;} function hole(a,b){return a===undefined && b===42;}";
    for (source, expected) in [
        ("sum(...values);", Value::Number(42.0)),
        ("new Ctor(...values).answer;", Value::Number(42.0)),
        ("sum(20,...'2');", Value::String("202".into())),
        ("shape(...'𝄞');", Value::String("1:2".into())),
        ("shape(...'\u{D7FF}');", Value::String("1:1".into())),
        ("shape(...'\\uD800');", Value::String("1:1".into())),
        ("hole(...holes);", Value::Boolean(true)),
    ] {
        for backend in [false, true] {
            let realm = setup(bootstrap);
            let result = if backend {
                value(&realm, source)
            } else {
                match StatementEvaluator::new(realm)
                    .evaluate_program(&parse_script(source).unwrap())
                    .unwrap()
                {
                    Completion::Normal(Some(value)) => value,
                    completion => panic!("{completion:?}"),
                }
            };
            assert!(result.same_value(&expected), "{source}: {result:?}");
        }
    }
}

#[test]
fn iterator_getters_next_receiver_and_done_value_order_are_observable() {
    let bootstrap = "var log=''; var index=0; var iterable={}; var it={}; Object.defineProperty(iterable,Symbol.iterator,{get:function(){log=log+'I';return function(){if(this!==iterable)throw 90;log=log+'C';return it;};}}); Object.defineProperty(it,'next',{get:function(){log=log+'N';return function(){if(this!==it)throw 91;log=log+'S';index=index+1;return {get done(){log=log+'D';return index===3;},get value(){log=log+'V';return index*10;}};};}}); function sum(a,b){log=log+'F';return a+b;}";
    for backend in [false, true] {
        let realm = setup(bootstrap);
        let result = if backend {
            vm(&realm, "sum(...iterable);")
        } else {
            StatementEvaluator::new(realm.clone())
                .evaluate_program(&parse_script("sum(...iterable);").unwrap())
        }
        .unwrap();
        assert!(matches!(
            result,
            Completion::Normal(Some(Value::Number(30.0)))
        ));
        assert!(value(&realm, "log;").same_value(&Value::String("ICNSDVSDVSDF".into())));
    }
}

#[test]
fn iterator_abrupt_completions_are_preserved_without_spurious_return_calls() {
    for expression in [
        "({get next(){throw 17;},return:function(){closed=closed+1;return {};}})",
        "({next:function(){throw 17;},return:function(){closed=closed+1;return {};}})",
        "({next:function(){return {get done(){throw 17;}};},return:function(){closed=closed+1;return {};}})",
        "({next:function(){return {done:false,get value(){throw 17;}};},return:function(){closed=closed+1;return {};}})",
    ] {
        let bootstrap = format!(
            "var closed=0; var iterable={{[Symbol.iterator]:function(){{return {expression};}}}}; function f(){{throw 99;}}"
        );
        for backend in [false, true] {
            let realm = setup(&bootstrap);
            let error = if backend {
                vm(&realm, "f(...iterable);")
            } else {
                StatementEvaluator::new(realm.clone())
                    .evaluate_program(&parse_script("f(...iterable);").unwrap())
            }
            .unwrap_err();
            assert_eq!(error.kind(), RuntimeErrorKind::ThrownValue);
            assert!(
                error
                    .thrown_value()
                    .unwrap()
                    .same_value(&Value::Number(17.0))
            );
            assert!(value(&realm, "closed;").same_value(&Value::Number(0.0)));
        }
    }
}

#[test]
fn spread_rejects_array_like_objects_and_invalid_protocol_records() {
    for expression in [
        "({length:1,0:42})",
        "({[Symbol.iterator]:1})",
        "({[Symbol.iterator]:function(){return 42;}})",
        "({[Symbol.iterator]:function(){return {next:0};}})",
        "({[Symbol.iterator]:function(){return {next:function(){return 42;}};}})",
    ] {
        let realm = setup(&format!(
            "var iterable={expression}; function f(){{return 42;}}"
        ));
        assert_eq!(
            vm(&realm, "f(...iterable);").unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
    }
    let realm = setup(
        "var nil=null; var count=0; var iterable={[Symbol.iterator]:function(){count=count+1;throw 17;}};",
    );
    assert!(value(&realm, "nil?.(...iterable);").same_value(&Value::Undefined));
    assert!(value(&realm, "count;").same_value(&Value::Number(0.0)));
    let error = vm(&realm, "1(...iterable);").unwrap_err();
    assert!(
        error
            .thrown_value()
            .unwrap()
            .same_value(&Value::Number(17.0))
    );
}

#[test]
fn array_iterator_alias_brand_completion_and_dynamic_length_are_preserved() {
    let realm = setup(
        "var a=[20]; var i=a.values(); var first=i.next().value; a[1]=22; var second=i.next().value; var end=i.next().done; a[2]=99; var stillDone=i.next().done;",
    );
    assert!(value(&realm, "first+second;").same_value(&Value::Number(42.0)));
    assert!(value(&realm, "end === stillDone;").same_value(&Value::Boolean(true)));
    assert!(
        value(
            &realm,
            "Array.prototype.values === Array.prototype[Symbol.iterator];"
        )
        .same_value(&Value::Boolean(true))
    );
    use temiya::runtime::{IntrinsicId, Object, PropertyKey};
    let next = realm
        .intrinsic(IntrinsicId::ArrayIteratorPrototype)
        .get(&PropertyKey::from("next"))
        .unwrap();
    let Value::Object(next) = next else {
        panic!("missing next")
    };
    assert_eq!(
        next.call(Value::Object(Object::new()), &[])
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
    let string_next = realm
        .intrinsic(IntrinsicId::StringIteratorPrototype)
        .get(&PropertyKey::from("next"))
        .unwrap();
    let Value::Object(string_next) = string_next else {
        panic!("missing next")
    };
    let array_iterator = value(&realm, "a.values();");
    assert_eq!(
        string_next.call(array_iterator, &[]).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn overridden_iterators_and_captured_next_override_array_like_storage() {
    let bootstrap = "var a=[99,99]; var count=0; var it={next:function(){count=count+1;it.next=function(){throw 99;};return {done:count===3,value:21};}}; a[Symbol.iterator]=function(){return it;}; function sum(a,b){return a+b;}";
    for backend in [false, true] {
        let realm = setup(bootstrap);
        let result = if backend {
            vm(&realm, "sum(...a);")
        } else {
            StatementEvaluator::new(realm.clone())
                .evaluate_program(&parse_script("sum(...a);").unwrap())
        }
        .unwrap();
        assert!(matches!(
            result,
            Completion::Normal(Some(Value::Number(42.0)))
        ));
        assert!(value(&realm, "count;").same_value(&Value::Number(3.0)));
    }
}

#[test]
fn array_next_reentrant_length_and_abrupt_elements_preserve_index_order() {
    let realm = setup(
        "var it;var first=true;var nested;var a={0:42,get length(){if(first){first=false;nested=it.next().value;}return 1;}};it=Array.prototype.values.call(a);var outer=it.next().value;",
    );
    assert!(value(&realm, "nested+outer;").same_value(&Value::Number(84.0)));
    assert!(value(&realm, "it.next().done;").same_value(&Value::Boolean(true)));
    let realm =
        setup("var a={length:2,get 0(){throw 17;},1:42};var it=Array.prototype.values.call(a);");
    let error = vm(&realm, "it.next();").unwrap_err();
    assert!(
        error
            .thrown_value()
            .unwrap()
            .same_value(&Value::Number(17.0))
    );
    assert!(value(&realm, "it.next().value;").same_value(&Value::Number(42.0)));
}

#[test]
fn argument_limit_closes_iterator_and_releases_pending_frames() {
    let realm = setup(
        "var closed=0;var iterable={[Symbol.iterator]:function(){return {next:function(){return {done:false,value:1};},return:function(){closed=closed+1;throw 17;}};}};function f(){return 42;}",
    );
    let error = vm(&realm, "f(...iterable);").unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::RangeError);
    assert!(value(&realm, "closed;").same_value(&Value::Number(1.0)));
    assert!(value(&realm, "f();").same_value(&Value::Number(42.0)));
}
