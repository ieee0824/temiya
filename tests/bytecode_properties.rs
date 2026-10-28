use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    runtime::{
        Completion, Realm, RuntimeError, RuntimeErrorKind, RuntimeResult, ScriptRecord, Value,
    },
};
fn execute(realm: &Realm, source: &str, vm: bool) -> RuntimeResult<Completion> {
    if vm {
        VirtualMachine::execute_in_realm(&BytecodeCompiler::compile(source).unwrap(), realm)
    } else {
        ScriptRecord::parse(realm.clone(), source)?.evaluate()
    }
}
fn value(result: RuntimeResult<Completion>) -> Value {
    match result.unwrap() {
        Completion::Normal(v) => v.unwrap_or(Value::Undefined),
        other => panic!("expected normal: {other:?}"),
    }
}
fn setup(realm: &Realm, source: &str) {
    value(execute(realm, source, false));
}
fn assert_value(actual: Value, expected: &Value) {
    assert!(actual.same_value(expected), "{actual:?} != {expected:?}");
}

#[test]
fn array_push_length_reads_keep_coercion_getters_and_setters_ordered() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(
            &realm,
            "var log='';var receiver={length:{valueOf:function(){log+='N';receiver.length=100;return 2;}}};Object.defineProperty(receiver,'2',{set:function(value){log+='I';if(this!==receiver||value!==7)throw 99;}});",
        );
        assert_value(
            value(execute(
                &realm,
                "var count=Array.prototype.push.call(receiver,7);log+':'+count+':'+receiver.length;",
                vm,
            )),
            &Value::String("NI:3:3".into()),
        );
        let realm = Realm::ordinary_in(&allocation_context);
        setup(&realm, "var log='';var receiver={get length(){log+='G';return 2;},set length(value){log+='L';if(value!==3)throw 99;}};Object.defineProperty(receiver,'2',{set:function(value){log+='I';if(this!==receiver||value!==7)throw 99;}});
");
        assert_value(
            value(execute(
                &realm,
                "Array.prototype.push.call(receiver,7);log;",
                vm,
            )),
            &Value::String("GIL".into()),
        );
    }
}

#[test]
fn literal_string_compound_assignment_preserves_captures_results_and_coercion() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        for (source, expected) in [
            (
                "function f(){var s='a';var first=(s+='b');var second=(s+='c');return first+':'+second+':'+s;}f();",
                "ab:abc:abc",
            ),
            (
                "function f(){var s={valueOf:function(){s='changed';return 'old';}};var r=(s+='!');return r+':'+s;}f();",
                "old!:old!",
            ),
            (
                "function f(){var s=41;var r=(s+='x');return r+':'+s;}f();",
                "41x:41x",
            ),
            ("function f(){let s='a';s+='';return s;}f();", "a"),
            ("function f(){let s='a';s+='b';return s;}f();", "ab"),
        ] {
            assert_value(
                value(execute(
                    &Realm::ordinary_in(&allocation_context),
                    source,
                    vm,
                )),
                &Value::String(expected.into()),
            );
        }
        assert_value(
            value(execute(
                &Realm::ordinary_in(&allocation_context),
                "(function(){var s='';var alias='';for(var i=0;i<200;i++){s+='ab';if(i===100)alias=s;}var second=s;for(var j=0;j<100;j++)s+='cd';return alias.length===202 && second.length===400 && s.length===600 && alias.charCodeAt(201)===98 && second.charCodeAt(399)===98 && s.charCodeAt(400)===99;})();",
                vm,
            )),
            &Value::Boolean(true),
        );
        for suffix in ["", "b"] {
            let realm = Realm::ordinary_in(&allocation_context);
            let source = format!(
                "var read;function f(){{const s='a';read=function(){{return s;}};s+='{suffix}';}}f();"
            );
            assert_eq!(
                execute(&realm, &source, vm).unwrap_err().kind(),
                RuntimeErrorKind::TypeError
            );
            assert_value(
                value(execute(&realm, "read();", vm)),
                &Value::String("a".into()),
            );
        }
    }
}

#[test]
fn primitive_string_concat_preserves_snapshots_utf16_and_evaluation_order() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        for source in [
            r#"var log='';function left(){log+='L';return 'A\ud800';}function right(){log+='R';return '\udc00B';}var joined=left()+right();joined.length===4 && joined.charCodeAt(1)===55296 && joined.charCodeAt(2)===56320 && log==='LR';"#,
            "var left='a';function right(){left='z';return 'b';}var joined=left+right();joined==='ab' && left==='z';",
            "var s='';var alias='';for(var i=0;i<200;i++){s+='ab';if(i===100)alias=s;}var appended=s+'!';s.length===400 && alias.length===202 && appended.length===401 && alias.charCodeAt(201)===98;",
            "var s='ab';var alias=s;s+='';s.length===2 && alias==='ab' && (s+'cd').length===4;",
        ] {
            assert_value(
                value(execute(
                    &Realm::ordinary_in(&allocation_context),
                    source,
                    vm,
                )),
                &Value::Boolean(true),
            );
        }
    }
}

#[test]
fn numeric_array_reads_preserve_canonical_keys_holes_and_getters() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(
            &realm,
            "var calls=0;var array=[11,undefined];array['-0']=90;array[-1]=91;array[1.5]=92;array[4294967295]=93;array[NaN]=94;array[Infinity]=95;Object.setPrototypeOf(array,{get 1(){calls++;return this[0]+20;}});",
        );
        assert_value(
            value(execute(
                &realm,
                "array[-0]===11 && array[1]===undefined && calls===0 && array['-0']===90 && array[-1]===91 && array[1.5]===92 && array[4294967295]===93 && array[4294967294]===undefined && array[NaN]===94 && array[Infinity]===95 && array.length===2;",
                vm,
            )),
            &Value::Boolean(true),
        );
        assert_value(
            value(execute(&realm, "delete array[1];array[1];", vm)),
            &Value::Number(31.0),
        );
        setup(
            &realm,
            "Object.defineProperty(array,'0',{get:function(){calls++;return 7;},configurable:true});",
        );
        assert_value(value(execute(&realm, "array[0];", vm)), &Value::Number(7.0));
        assert_value(value(execute(&realm, "calls;", vm)), &Value::Number(2.0));
    }
}

#[test]
fn numeric_string_reads_return_utf16_units_and_keep_prototype_fallback_receiver() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(
            &realm,
            "var calls=0;var s='A😀';Object.defineProperty(String.prototype,'0',{get:function(){calls++;return 'wrong';},configurable:true});Object.defineProperty(String.prototype,'4',{get:function(){'use strict';calls++;return typeof this+':'+this.length;},configurable:true});",
        );
        assert_value(
            value(execute(
                &realm,
                "s[-0]==='A' && s[1].charCodeAt(0)===55357 && s[2].charCodeAt(0)===56832 && s[3]===undefined && s['-0']===undefined && s[1.5]===undefined && s[4]==='string:3' && calls===1;",
                vm,
            )),
            &Value::Boolean(true),
        );
    }
}

#[test]
fn loop_named_reads_observe_reordering_accessors_and_prototype_changes() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        let source = "var log='';var obj={prefix:0,x:1};var sum=0;for(var i=0;i<4;i++){sum+=obj.x;if(i===0){delete obj.prefix;obj.other=99;}if(i===1){delete obj.x;Object.defineProperty(obj,'x',{get:function(){log+='G';return this.other===99?4:400;},configurable:true});}if(i===2){delete obj.x;Object.setPrototypeOf(obj,{x:8});}}sum+':'+log;";
        assert_value(
            value(execute(&realm, source, vm)),
            &Value::String("14:G".into()),
        );
    }
}

#[test]
fn static_and_computed_method_names_retain_receiver_across_getter_reentry() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        for call in ["obj.method()", "obj['method']()"] {
            let realm = Realm::ordinary_in(&allocation_context);
            setup(
                &realm,
                "var log='';var other={x:2};var obj={x:1,get method(){log+='G';obj=other;return function(){return this.x;};}};",
            );
            assert_value(
                value(execute(
                    &realm,
                    &format!("var r={call};log+':'+r+':'+obj.x;"),
                    vm,
                )),
                &Value::String("G:1:2".into()),
            );
        }
    }
}

#[test]
fn member_reads_writes_and_parentheses_match_ast() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(
            &realm,
            "var obj={x:1,get y(){log=log+'g';return this.x;},set y(v){log=log+'s';this.x=v;}}; var log='';",
        );
        assert_value(
            value(execute(&realm, "(obj.y)=5; obj['y'];", vm)),
            &Value::Number(5.0),
        );
        assert_value(
            value(execute(&realm, "log;", false)),
            &Value::String("sg".into()),
        );
        assert_value(
            value(execute(&realm, "obj[1n]=7; obj[1];", vm)),
            &Value::Number(7.0),
        );
        assert_value(
            value(execute(&realm, "obj.nested=obj; obj.nested.x;", vm)),
            &Value::Number(5.0),
        );
        assert_value(
            value(execute(&realm, "this.obj.x;", vm)),
            &Value::Number(5.0),
        );
    }
}

#[test]
fn optional_members_preserve_chain_boundaries_and_skip_key_evaluation() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(
            &realm,
            "var absent=null; var count=0; var obj={x:{value:7}};",
        );
        assert_value(
            value(execute(&realm, "absent?.[(count=count+1)].next;", vm)),
            &Value::Undefined,
        );
        assert_value(
            value(execute(
                &realm,
                "delete absent?.[(count=count+1)].next;",
                vm,
            )),
            &Value::Boolean(true),
        );
        assert_value(value(execute(&realm, "count;", false)), &Value::Number(0.0));
        assert_value(
            value(execute(&realm, "obj?.x?.value;", vm)),
            &Value::Number(7.0),
        );
        assert_eq!(
            execute(&realm, "(absent?.x).next;", vm).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
        assert_value(
            value(execute(&realm, "delete obj?.x.value;", vm)),
            &Value::Boolean(true),
        );
        assert_value(
            value(execute(&realm, "obj.x.value;", vm)),
            &Value::Undefined,
        );
    }
}

#[test]
fn computed_key_expression_rhs_and_coercion_have_correct_order() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(
            &realm,
            "var log=''; var old={}; var obj=old; var key={toString:function(){log=log+'K';return 'x';}}; var rhs={get value(){log=log+'R';obj={};return 9;}};",
        );
        assert_value(
            value(execute(&realm, "obj[key]=rhs.value; old.x;", vm)),
            &Value::Number(9.0),
        );
        assert_value(
            value(execute(&realm, "log;", false)),
            &Value::String("RK".into()),
        );
        setup(
            &realm,
            "var missing=null; var step=0; var coercions=0; var badKey={toString:function(){coercions=coercions+1;return 'x';}};",
        );
        assert_eq!(
            execute(&realm, "missing[(step=step+1,badKey)];", vm)
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::TypeError
        );
        assert_value(value(execute(&realm, "step;", false)), &Value::Number(1.0));
        assert_value(
            value(execute(&realm, "coercions;", false)),
            &Value::Number(0.0),
        );
        assert_eq!(
            execute(&realm, "missing[badKey]=(step=step+1);", vm)
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::TypeError
        );
        assert_value(value(execute(&realm, "step;", false)), &Value::Number(2.0));
        assert_value(
            value(execute(&realm, "coercions;", false)),
            &Value::Number(0.0),
        );
    }
}

#[test]
fn deletion_does_not_get_the_property_and_obeys_strict_failures() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(
            &realm,
            "var count=0; var obj={get value(){count=count+1;return 2;}}; Object.defineProperty(obj,'fixed',{value:1,configurable:false});",
        );
        assert_value(
            value(execute(&realm, "delete (obj.value);", vm)),
            &Value::Boolean(true),
        );
        assert_value(value(execute(&realm, "count;", false)), &Value::Number(0.0));
        assert_value(
            value(execute(&realm, "delete obj.fixed;", vm)),
            &Value::Boolean(false),
        );
        assert_eq!(
            execute(&realm, "'use strict'; delete obj.fixed;", vm)
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::TypeError
        );
        assert_value(
            value(execute(&realm, "delete 'abc'[0];", vm)),
            &Value::Boolean(false),
        );
        assert_eq!(
            execute(&realm, "'use strict'; delete 'abc'.length;", vm)
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::TypeError
        );
        assert_value(
            value(execute(&realm, "delete nonexistent;", vm)),
            &Value::Boolean(true),
        );
    }
}

#[test]
fn primitive_prototypes_and_property_assignment_failures_match() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(
            &realm,
            "Object.defineProperty(String.prototype,'receiver',{get:function(){'use strict';return this;}});",
        );
        assert_value(
            value(execute(&realm, "'abc'.receiver;", vm)),
            &Value::String("abc".into()),
        );
        assert_value(
            value(execute(&realm, "'a😀'.length;", vm)),
            &Value::Number(3.0),
        );
        assert_value(
            value(execute(&realm, "'abc'.extra=4;", vm)),
            &Value::Number(4.0),
        );
        assert_eq!(
            execute(&realm, "'use strict'; 'abc'.extra=4;", vm)
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::TypeError
        );
    }
}

#[test]
fn proxy_receiver_symbol_keys_and_abrupt_deletion_are_preserved() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::{cell::RefCell, rc::Rc};
    use temiya::runtime::{Object, ProxyObject, Symbol};
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        let symbol = Symbol::new(Some("key".into()));
        let target = Object::new_in(&allocation_context);
        target
            .create_data_property(symbol.clone().into(), Value::Number(7.0))
            .unwrap();
        let receiver = Rc::new(RefCell::new(None::<Object>));
        let seen = receiver.clone();
        let handler = Object::new_in(&allocation_context);
        handler
            .create_data_property(
                "get".into(),
                Value::Object(Object::native_function_in(
                    &allocation_context,
                    move |_, args| {
                        assert!(args[2].same_value(&Value::Object(seen.borrow().clone().unwrap())));
                        let Value::Object(target) = &args[0] else {
                            panic!()
                        };
                        target.get(&args[1].to_property_key()?)
                    },
                )),
            )
            .unwrap();
        handler
            .create_data_property(
                "deleteProperty".into(),
                Value::Object(Object::native_function_in(&allocation_context, |_, _| {
                    Err(RuntimeError::thrown(Value::Number(17.0)))
                })),
            )
            .unwrap();
        let set_receiver = receiver.clone();
        handler
            .create_data_property(
                "set".into(),
                Value::Object(Object::native_function_in(
                    &allocation_context,
                    move |_, args| {
                        assert!(
                            args[3]
                                .same_value(&Value::Object(set_receiver.borrow().clone().unwrap()))
                        );
                        Ok(Value::Boolean(false))
                    },
                )),
            )
            .unwrap();
        let proxy = ProxyObject::new(target, handler);
        *receiver.borrow_mut() = Some(proxy.object());
        realm
            .global_object()
            .create_data_property("obj".into(), Value::Object(proxy.object()))
            .unwrap();
        realm
            .global_object()
            .create_data_property("sym".into(), Value::Symbol(symbol))
            .unwrap();
        assert_value(value(execute(&realm, "obj[sym];", vm)), &Value::Number(7.0));
        assert_value(
            value(execute(&realm, "obj[sym]=9;", vm)),
            &Value::Number(9.0),
        );
        assert_eq!(
            execute(&realm, "'use strict'; obj[sym]=9;", vm)
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::TypeError
        );
        assert_value(value(execute(&realm, "obj[sym];", vm)), &Value::Number(7.0));
        let error = execute(&realm, "delete obj[sym];", vm).unwrap_err();
        assert_eq!(error.kind(), RuntimeErrorKind::ThrownValue);
        assert!(
            error
                .thrown_value()
                .unwrap()
                .same_value(&Value::Number(17.0))
        );
        receiver.borrow_mut().take();
    }
}

#[test]
fn primitive_inherited_setters_keep_the_original_receiver() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(
            &realm,
            "var log=''; Object.defineProperty(Number.prototype,'sink',{set:function(v){'use strict';log=(typeof this)+v;}});",
        );
        assert_value(
            value(execute(&realm, "'use strict'; (9).sink=4;", vm)),
            &Value::Number(4.0),
        );
        assert_value(
            value(execute(&realm, "log;", false)),
            &Value::String("number4".into()),
        );
    }
}

#[test]
fn supplied_global_this_and_function_receiver_match_ast() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::{
        ast::StmtKind,
        parser::parse_script,
        runtime::{Environment, Object, StatementEvaluator, ThisBindingStatus},
    };
    let global = Object::new_in(&allocation_context);
    let receiver = Object::new_in(&allocation_context);
    receiver
        .create_data_property("answer".into(), Value::Number(42.0))
        .unwrap();
    let realm = Realm::new(global, Value::Object(receiver.clone()));
    assert_value(
        value(execute(&realm, "this.answer;", false)),
        &Value::Number(42.0),
    );
    assert_value(
        value(execute(&realm, "this.answer;", true)),
        &Value::Number(42.0),
    );
    let program = parse_script("function method(){return this.answer;}").unwrap();
    StatementEvaluator::new(realm.clone())
        .evaluate_program(&program)
        .unwrap();
    let StmtKind::FunctionDeclaration(function) = &program.body[0].kind else {
        panic!()
    };
    let Value::Object(object) = realm
        .global_environment()
        .get_identifier_value(&"method".into(), true)
        .unwrap()
    else {
        panic!()
    };
    assert_value(
        object.call(Value::Object(receiver.clone()), &[]).unwrap(),
        &Value::Number(42.0),
    );
    let frame = Environment::function(
        Some(realm.global_environment()),
        object,
        None,
        ThisBindingStatus::Uninitialized,
    );
    frame.bind_this_value(Value::Object(receiver)).unwrap();
    let code = BytecodeCompiler::compile_function_body(function).unwrap();
    let Completion::Return(v) = VirtualMachine::execute_in_function(&code, &realm, &frame).unwrap()
    else {
        panic!()
    };
    assert_value(v, &Value::Number(42.0));
}

#[test]
fn javascript_exotic_key_conversion_receives_string_hint_and_preserves_symbols() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::runtime::{Symbol, WellKnownSymbol};
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(
            &realm,
            "var log=''; var key={}; var obj={}; var convert=function(hint){if(this!==key) throw 1; log=hint; return sym;};",
        );
        let symbol = Symbol::new(Some("property".into()));
        realm
            .global_object()
            .create_data_property("sym".into(), Value::Symbol(symbol.clone()))
            .unwrap();
        let Value::Object(key) = realm
            .global_environment()
            .get_identifier_value(&"key".into(), true)
            .unwrap()
        else {
            panic!()
        };
        let convert = realm
            .global_environment()
            .get_identifier_value(&"convert".into(), true)
            .unwrap();
        key.create_data_property(
            realm.well_known_symbol(WellKnownSymbol::ToPrimitive).into(),
            convert,
        )
        .unwrap();
        assert_value(
            value(execute(&realm, "obj[key]=42; obj[sym];", vm)),
            &Value::Number(42.0),
        );
        assert_value(
            value(execute(&realm, "log;", false)),
            &Value::String("string".into()),
        );
        setup(&realm, "var bad={toString:function(){throw 19;}};");
        let error = execute(&realm, "obj[bad];", vm).unwrap_err();
        assert_eq!(error.kind(), RuntimeErrorKind::ThrownValue);
        assert!(
            error
                .thrown_value()
                .unwrap()
                .same_value(&Value::Number(19.0))
        );
    }
}

#[test]
fn nested_references_and_reentrant_getters_keep_independent_stacks() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::runtime::{Object, PropertyDescriptor};
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        setup(&realm, "var first={}; var second={}; var obj={value:0};");
        assert_value(
            value(execute(&realm, "first.x=second.y=7; first.x+second.y;", vm)),
            &Value::Number(14.0),
        );
        let Value::Object(object) = realm
            .global_environment()
            .get_identifier_value(&"obj".into(), true)
            .unwrap()
        else {
            panic!()
        };
        let nested = BytecodeCompiler::compile("obj.value=9; obj.value;").unwrap();
        let callback_realm = realm.clone();
        object
            .define_own_property(
                "reenter".into(),
                PropertyDescriptor::accessor(
                    Some(Object::native_function_in(
                        &allocation_context,
                        move |_, _| match VirtualMachine::execute_in_realm(
                            &nested,
                            &callback_realm,
                        )? {
                            Completion::Normal(Some(value)) => Ok(value),
                            _ => Err(RuntimeError::internal_error("unexpected nested completion")),
                        },
                    )),
                    None,
                    true,
                    true,
                ),
            )
            .unwrap();
        assert_value(
            value(execute(
                &realm,
                "first.x=obj.reenter; first.x+obj.value;",
                vm,
            )),
            &Value::Number(18.0),
        );
        assert!(object.delete(&"reenter".into()));
        assert_value(
            value(execute(&realm, "''?.length;", vm)),
            &Value::Number(0.0),
        );
    }
}

#[test]
#[ignore = "requires pinned Test262 corpus; records current frontend gaps without fallback"]
fn pinned_test262_property_audit() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::process::Command;
    use temiya::test262::{TEST262_REVISION, TestCase};
    let corpus = std::env::var("TEST262_DIR").expect("TEST262_DIR required");
    let read = |path: &str| {
        let output = Command::new("git")
            .args(["-C", &corpus, "show", &format!("{TEST262_REVISION}:{path}")])
            .output()
            .unwrap();
        assert!(output.status.success(), "missing pinned {path}");
        String::from_utf8(output.stdout).unwrap()
    };
    let paths = [
        "delete/member-computed-reference-null.js",
        "delete/member-computed-reference-undefined.js",
        "delete/member-identifier-reference-null.js",
        "delete/member-identifier-reference-undefined.js",
        "delete/non-reference-return-true.js",
        "delete/11.4.1-4-a-1-s.js",
        "delete/11.4.1-4-a-2-s.js",
        "delete/11.4.1-4.a-8-s.js",
        "property-accessors/S11.2.1_A1.1.js",
        "optional-chaining/short-circuiting.js",
        "optional-chaining/optional-chain-prod-identifiername.js",
    ];
    let mut outcomes = vec![];
    for relative in paths {
        let path = format!("test/language/expressions/{relative}");
        let case = TestCase::parse(&path, read(&path)).unwrap();
        assert!(case.metadata.negative.is_none());
        assert!(
            !case
                .metadata
                .flags
                .iter()
                .any(|f| matches!(f.as_str(), "module" | "async" | "raw"))
        );
        for variant in case.variants() {
            let source = case.source_for(variant);
            for vm in [false, true] {
                let backend = if vm { "bytecode" } else { "AST" };
                let code = if vm {
                    match BytecodeCompiler::compile(&source) {
                        Ok(code) => Some(code),
                        Err(error) => {
                            outcomes.push(serde_json::json!({"path":path,"variant":format!("{variant:?}"),"backend":backend,"status":"compile_unsupported","message":error.to_string()}));
                            continue;
                        }
                    }
                } else {
                    None
                };
                let run = || -> Result<(), String> {
                    let realm = Realm::ordinary_in(&allocation_context);
                    for harness in ["sta.js", "assert.js"]
                        .into_iter()
                        .chain(case.metadata.includes.iter().map(String::as_str))
                    {
                        ScriptRecord::parse(realm.clone(), &read(&format!("harness/{harness}")))
                            .map_err(|e| format!("harness parse {harness}: {e}"))?
                            .evaluate()
                            .map_err(|e| format!("harness evaluate {harness}: {e:?}"))?;
                    }
                    let result = if let Some(code) = &code {
                        VirtualMachine::execute_in_realm(code, &realm)
                    } else {
                        ScriptRecord::parse(realm, &source)
                            .map_err(|e| format!("parse: {e}"))?
                            .evaluate()
                    };
                    match result {
                        Ok(Completion::Normal(_)) => Ok(()),
                        other => Err(format!("evaluation: {other:?}")),
                    }
                };
                let result = run();
                outcomes.push(serde_json::json!({"path":path,"variant":format!("{variant:?}"),"backend":backend,"status":if result.is_ok(){"pass"}else{"fail"},"message":result.err()}));
            }
        }
    }
    let totals = |backend: &str| {
        let filtered: Vec<_> = outcomes
            .iter()
            .filter(|r| r["backend"] == backend)
            .collect();
        serde_json::json!({"pass":filtered.iter().filter(|r|r["status"]=="pass").count(),"fail":filtered.iter().filter(|r|r["status"]=="fail").count(),"compile_unsupported":filtered.iter().filter(|r|r["status"]=="compile_unsupported").count()})
    };
    let ast = totals("AST");
    let bytecode = totals("bytecode");
    if let Ok(path) = std::env::var("TEST262_REPORT") {
        std::fs::write(path,serde_json::to_string_pretty(&serde_json::json!({"revision":TEST262_REVISION,"selected_files":paths.len(),"AST":ast,"bytecode":bytecode,"outcomes":outcomes,"scope":"Original whole fixtures and harness. No AST fallback for bytecode compilation failures. Unsupported is not pass."})).unwrap()).unwrap();
    }
    println!("AST: {ast}; bytecode: {bytecode}");
    assert!(
        outcomes.iter().all(|r| r["status"] == "pass"),
        "upstream integration remains incomplete; unsupported compilation is not pass"
    );
}
#[test]
fn ordinary_array_push_keeps_prototype_setters_readonly_lengths_and_proxy_order() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for (setup, source, expected) in [
        (
            "var a=[1,,3];a.length=1000000;",
            "a.push(42)===1000001 && a[1000000]===42 && !(1 in a) && Object.getOwnPropertyDescriptor(a,'1000000').writable;",
            Value::Boolean(true),
        ),
        (
            "var seen=0;Object.defineProperty(Array.prototype,'0',{set:function(v){seen=v;}});var a=[];",
            "a.push(42)===1 && seen===42 && Object.getOwnPropertyDescriptor(a,'0')===undefined;",
            Value::Boolean(true),
        ),
        (
            "var seen=0;Object.defineProperty(Object.prototype,'0',{set:function(v){seen=v;}});var a=[];",
            "a.push(42)===1 && seen===42 && Object.getOwnPropertyDescriptor(a,'0')===undefined;",
            Value::Boolean(true),
        ),
        (
            "var seen=0;Object.defineProperty(Array.prototype,'0',{set:function(v){seen=v;}});var a=[];Object.preventExtensions(a);",
            "a.push(42)===1 && seen===42 && Object.getOwnPropertyDescriptor(a,'0')===undefined;",
            Value::Boolean(true),
        ),
        (
            "var a=[];Object.preventExtensions(a);",
            "a.push()===0;",
            Value::Boolean(true),
        ),
        (
            "var seen=0;var a=[];Object.setPrototypeOf(a,new Proxy({},{set:function(t,k,v,r){seen=v;return true;}}));",
            "Array.prototype.push.call(a,42)===1 && seen===42 && Object.getOwnPropertyDescriptor(a,'0')===undefined;",
            Value::Boolean(true),
        ),
        (
            "var trace='';var a=[];var p=new Proxy(a,{set:function(t,k,v,r){trace+=k+',';t[k]=v;return true;}});",
            "Array.prototype.push.call(p,42);trace+':'+a.length+':'+a[0];",
            Value::String("0,length,:1:42".into()),
        ),
    ] {
        for vm in [false, true] {
            let realm = Realm::ordinary_in(&allocation_context);
            execute(&realm, setup, false).unwrap();
            assert!(
                value(execute(&realm, source, vm)).same_value(&expected),
                "{setup} {source}"
            );
        }
    }
    for vm in [false, true] {
        for source in ["a.push();", "a.push(42);"] {
            let realm = Realm::ordinary_in(&allocation_context);
            execute(&realm, "var seen=0;Object.defineProperty(Array.prototype,'0',{set:function(v){seen=v;}});var a=[];Object.defineProperty(a,'length',{writable:false});", false).unwrap();
            assert_eq!(
                execute(&realm, source, vm).unwrap_err().kind(),
                RuntimeErrorKind::TypeError
            );
            assert!(value(execute(&realm, "a.length;", vm)).same_value(&Value::Number(0.0)));
            assert!(
                value(execute(&realm, "seen;", vm)).same_value(&Value::Number(
                    if source == "a.push();" { 0.0 } else { 42.0 }
                ))
            );
        }
        let realm = Realm::ordinary_in(&allocation_context);
        execute(&realm, "var a=[];a.length=4294967295;", false).unwrap();
        assert_eq!(
            execute(&realm, "a.push(42);", vm).unwrap_err().kind(),
            RuntimeErrorKind::RangeError
        );
        assert!(
            value(execute(
                &realm,
                "a['4294967295']===42 && a.length===4294967295;",
                vm
            ))
            .same_value(&Value::Boolean(true))
        );
    }
}
