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
    let allocation_context = temiya::runtime::AllocationContext::new();
    for source in [
        "(function(a,b){return a+b;})(20,22);",
        "((a,b)=>a+b)(20,22);",
        "((a,b)=>{return a+b;})(20,22);",
        "(function(a){var a;return a;})(42);",
        "(function(a,a){return a;})(1,42);",
        "(function(){return arguments[0]+arguments[1];})(20,22);",
    ] {
        assert!(
            value(&Realm::ordinary_in(&allocation_context), source)
                .same_value(&Value::Number(42.0)),
            "{source}"
        );
    }
}

#[test]
fn prepared_parameter_frames_preserve_duplicates_missing_arguments_and_captures() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for source in [
        "(function(a,a){return a;})(1,42);",
        "(function(a,a){return typeof a==='undefined'?42:0;})(1);",
        "(function(a){var a;return a;})(42,99);",
        "(function(arguments){return arguments;})(42);",
        "(function(a){function a(){return 42;}return a();})(0);",
        "(function(a){return ()=>a;})(42)();",
        "((a,b)=>a+b)(20,22);",
        "function F(a){this.x=a;}var object=new F(42);object.x;",
        "(function(a){return arguments.length===2 && a===20?42:0;})(20,22);",
    ] {
        assert!(
            value(&Realm::ordinary_in(&allocation_context), source)
                .same_value(&Value::Number(42.0)),
            "{source}"
        );
    }
}

#[test]
fn arguments_remain_visible_to_escaped_arrows_and_independent_calls() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for source in [
        "var f=(function(a){return ()=>arguments[0];})(42);f();",
        "var f=(function(a){return ()=>()=>arguments[0];})(42);f()();",
        "function outer(a){function inner(){return ()=>arguments[0];}return inner(a);}outer(42)();",
        "function f(a){return ()=>arguments[0];}var x=f(20);var y=f(22);x()+y();",
        "(function(arguments){return ()=>arguments;})(42)();",
        "(function(){let arguments=42;return ()=>arguments;})()();",
        "(function(){'use strict';return arguments[0];})(42);",
        "(function(){return ()=>typeof arguments;})()() === 'object' ? 42 : 0;",
        "(function(){var arguments;return arguments[0];})(42);",
        "(function(){return arguments[0];var arguments;})(42);",
        "(function(){if(true){var arguments;}return arguments[0];})(42);",
        "(function(arguments){var arguments;return arguments;})(42);",
        "(function(){var arguments=42;return arguments;})();",
        "(function(){function arguments(){return 42;}return arguments();})();",
    ] {
        assert!(
            value(&Realm::ordinary_in(&allocation_context), source)
                .same_value(&Value::Number(42.0)),
            "{source}"
        );
    }
}

#[test]
fn escaped_captures_share_mutations_and_outlive_code_and_realm_handles() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let closure = {
        let realm = Realm::ordinary_in(&allocation_context);
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
    let allocation_context = temiya::runtime::AllocationContext::new();
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
            value(&Realm::ordinary_in(&allocation_context), source).same_value(&expected),
            "{source}"
        );
    }
    let global = Object::new_in(&allocation_context);
    let this = Object::new_in(&allocation_context);
    this.create_data_property("answer".into(), Value::Number(42.0))
        .unwrap();
    let realm = Realm::new(global, Value::Object(this));
    assert!(value(&realm, "(function(){return this.answer;})();").same_value(&Value::Number(42.0)));
    assert!(value(&realm, "(()=>this.answer)();").same_value(&Value::Number(42.0)));
}

#[test]
fn named_recursion_and_per_iteration_block_captures_are_real_environments() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    assert!(
        value(
            &Realm::ordinary_in(&allocation_context),
            "(function self(n){if(n===0){return 42;}return self(n-1);})(10);"
        )
        .same_value(&Value::Number(42.0))
    );
    let source = "let i=0;let run=true;let first;let second;while(run){let snapshot=i;if(i===0){first=()=>snapshot;}else{second=()=>snapshot;run=false;}i=i+1;}first()*100+second();";
    assert!(
        value(&Realm::ordinary_in(&allocation_context), source).same_value(&Value::Number(1.0))
    );
    assert!(
        value(
            &Realm::ordinary_in(&allocation_context),
            "(function self(){self=0;return self.name;})();"
        )
        .same_value(&Value::String("self".into()))
    );
}

#[test]
fn captured_tdz_const_throw_and_recursion_failure_preserve_values() {
    let allocation_context = temiya::runtime::AllocationContext::new();
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
        let realm = Realm::ordinary_in(&allocation_context);
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
    let realm = Realm::ordinary_in(&allocation_context);
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
        "(function(a){let a=42;return a;})(1);",
    ] {
        assert!(BytecodeCompiler::compile(source).is_err(), "{source}");
    }
}

#[test]
fn sloppy_primitive_this_uses_actual_wrapper_slots_and_prototypes() {
    let allocation_context = temiya::runtime::AllocationContext::new();
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
            value(&Realm::ordinary_in(&allocation_context), source).same_value(&expected),
            "{source}"
        );
    }
    let realm = Realm::ordinary_in(&allocation_context);
    let Value::Object(boxed) = value(&realm, "(function(){return this;}).call(42n);") else {
        panic!("BigInt this was not boxed")
    };
    assert_eq!(
        boxed.prototype(),
        Some(realm.intrinsic(temiya::runtime::IntrinsicId::BigIntPrototype))
    );
}

#[test]
fn declarations_are_hoisted_and_last_duplicates_win_without_private_self_binding() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for source in [
        "f(21); function f(x){return x*2;}",
        "f(); function f(){return 1;} function f(){return 42;}",
        "function f(){return f;} var old=f;f=42;old();",
        "function outer(f){function f(){return 42;}return f();}outer(0);",
        "function outer(){return f();function f(){return 42;}}outer();",
        "function outer(){function get(){return value;}let value=42;return get;}outer()();",
        "function outer(){'use strict';function get(){return value;}let value=42;return get;}outer()();",
        "function outer(){function arguments(){return 42;}return arguments();}outer();",
        "function outer(){let arguments=42;return arguments;}outer();",
        "'use strict';let answer;{answer=f();function f(){return 42;}}answer;",
    ] {
        assert!(
            value(&Realm::ordinary_in(&allocation_context), source)
                .same_value(&Value::Number(42.0)),
            "{source}"
        );
    }
    for source in [
        "function outer(){return get();let value=42;function get(){return value;}}outer();",
        "function outer(){'use strict';return get();let value=42;function get(){return value;}}outer();",
    ] {
        let code = BytecodeCompiler::compile(source).unwrap();
        assert_eq!(
            VirtualMachine::execute(&code).unwrap_err().kind(),
            RuntimeErrorKind::ReferenceError
        );
    }
    for source in [
        "{function f(){return 42;}}",
        "if(true) function f(){return 42;}",
    ] {
        assert!(BytecodeCompiler::compile(source).is_err(), "{source}");
    }
}

#[test]
fn global_function_preflight_is_atomic_before_lexical_var_or_function_creation() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::runtime::PropertyDescriptor;
    for mode in 0..3 {
        let realm = Realm::ordinary_in(&allocation_context);
        let global = realm.global_object();
        if mode == 0 {
            global
                .define_own_property(
                    "blocked".into(),
                    PropertyDescriptor::data(Value::Number(7.0), false, true, false),
                )
                .unwrap();
        } else if mode == 1 {
            value(&realm, "let blocked=7;");
        } else {
            global
                .create_data_property("blocked".into(), Value::Number(7.0))
                .unwrap();
            global.prevent_extensions();
        }
        let code =
            BytecodeCompiler::compile("let fresh=1;var unrelated;function blocked(){return 42;}")
                .unwrap();
        let error = VirtualMachine::execute_in_realm(&code, &realm).unwrap_err();
        assert_eq!(
            error.kind(),
            if mode == 1 {
                RuntimeErrorKind::SyntaxError
            } else {
                RuntimeErrorKind::TypeError
            }
        );
        assert!(
            !realm
                .global_environment()
                .has_lexical_declaration(&"fresh".into())
                .unwrap()
        );
        assert!(
            global
                .try_get_own_property(&"unrelated".into())
                .unwrap()
                .is_none()
        );
        if mode != 1 {
            assert!(
                global
                    .get(&"blocked".into())
                    .unwrap()
                    .same_value(&Value::Number(7.0))
            );
        }
    }
}

#[test]
fn global_functions_precede_vars_in_selected_source_order_and_replace_accessors() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::{cell::Cell, rc::Rc};
    use temiya::{
        parser::parse_script,
        runtime::{PropertyDescriptor, PropertyKey, StatementEvaluator},
    };
    let source = "var before;function a(){return 1;}function b(){return 2;}function a(){return 42;}var after;a();";
    for vm_backend in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        if vm_backend {
            assert!(value(&realm, source).same_value(&Value::Number(42.0)));
        } else {
            StatementEvaluator::new(realm.clone())
                .evaluate_program(&parse_script(source).unwrap())
                .unwrap();
        }
        let keys = realm
            .global_object()
            .try_own_property_keys()
            .unwrap()
            .into_iter()
            .filter(|key| {
                ["a", "b", "before", "after"]
                    .iter()
                    .any(|name| key == &PropertyKey::from(*name))
            })
            .collect::<Vec<_>>();
        assert_eq!(keys, ["b", "a", "before", "after"].map(PropertyKey::from));
    }
    let realm = Realm::ordinary_in(&allocation_context);
    let calls = Rc::new(Cell::new(0));
    let captured = calls.clone();
    let getter = Object::native_function_in(&allocation_context, move |_, _| {
        captured.set(captured.get() + 1);
        Ok(Value::Undefined)
    });
    realm
        .global_object()
        .define_own_property(
            "f".into(),
            PropertyDescriptor::accessor(Some(getter), None, true, true),
        )
        .unwrap();
    assert!(value(&realm, "function f(){return 42;}f();").same_value(&Value::Number(42.0)));
    assert_eq!(calls.get(), 0);
    let descriptor = realm
        .global_object()
        .try_get_own_property(&"f".into())
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            descriptor.writable,
            descriptor.enumerable,
            descriptor.configurable
        ),
        (Some(true), Some(true), Some(false))
    );
}

#[test]
#[ignore = "requires pinned Test262 corpus; records unfinished closure integration gates"]
fn pinned_test262_closure_audit() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::{collections::BTreeMap, process::Command};
    use temiya::{
        runtime::ScriptRecord,
        test262::{TEST262_REVISION, TestCase},
    };
    let corpus = std::env::var("TEST262_DIR").expect("TEST262_DIR required");
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-c")
            .arg(format!("safe.directory={corpus}"))
            .arg("-C")
            .arg(&corpus)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git read failed: {:?}",
            output.stderr
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let read = |path: &str| git(&["show", &format!("{TEST262_REVISION}:{path}")]);
    let paths = [
        "expressions/function/scope-name-var-close.js",
        "expressions/function/scope-name-var-open-non-strict.js",
        "expressions/function/scope-name-var-open-strict.js",
        "expressions/function/scope-body-lex-distinct.js",
        "expressions/function/named-strict-error-reassign-fn-name-in-body-in-arrow.js",
        "expressions/function/named-no-strict-reassign-fn-name-in-body-in-arrow.js",
        "expressions/function/dflt-params-ref-prior.js",
        "expressions/function/scope-param-rest-elem-var-close.js",
        "expressions/arrow-function/lexical-this.js",
        "expressions/arrow-function/lexical-new.target-closure-returned.js",
        "statements/function/scope-body-lex-distinct.js",
        "statements/function/dflt-params-ref-prior.js",
    ];
    let mut blobs = BTreeMap::new();
    let mut outcomes = vec![];
    for relative in paths {
        let path = format!("test/language/{relative}");
        blobs.insert(
            path.clone(),
            git(&["rev-parse", &format!("{TEST262_REVISION}:{path}")])
                .trim()
                .to_string(),
        );
        let case = TestCase::parse(&path, read(&path)).unwrap();
        assert!(case.metadata.negative.is_none());
        assert!(
            !case
                .metadata
                .flags
                .iter()
                .any(|flag| matches!(flag.as_str(), "module" | "async" | "raw"))
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
                            .map_err(|e| format!("test parse: {e}"))?
                            .evaluate()
                    };
                    match result {
                        Ok(Completion::Normal(_)) => Ok(()),
                        Err(error) => {
                            let detail = if let Some(Value::Object(object)) = error.thrown_value() {
                                if let Ok(Value::String(message)) = object.get(&"message".into()) {
                                    message.to_string_lossy()
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            };
                            Err(format!("evaluation: {error:?}; thrown message: {detail}"))
                        }
                        other => Err(format!("evaluation: {other:?}")),
                    }
                };
                let result = run();
                outcomes.push(serde_json::json!({"path":path,"variant":format!("{variant:?}"),"backend":backend,"status":if result.is_ok(){"pass"}else{"fail"},"message":result.err()}));
            }
        }
        for harness in ["sta.js", "assert.js"]
            .into_iter()
            .chain(case.metadata.includes.iter().map(String::as_str))
        {
            let path = format!("harness/{harness}");
            blobs.insert(
                path.clone(),
                git(&["rev-parse", &format!("{TEST262_REVISION}:{path}")])
                    .trim()
                    .to_string(),
            );
        }
    }
    let totals = |backend: &str| {
        let rows = outcomes
            .iter()
            .filter(|row| row["backend"] == backend)
            .collect::<Vec<_>>();
        serde_json::json!({"pass":rows.iter().filter(|row|row["status"]=="pass").count(),"fail":rows.iter().filter(|row|row["status"]=="fail").count(),"compile_unsupported":rows.iter().filter(|row|row["status"]=="compile_unsupported").count(),"skipped":0})
    };
    let ast = totals("AST");
    let bytecode = totals("bytecode");
    if let Ok(path) = std::env::var("TEST262_REPORT") {
        std::fs::write(path, serde_json::to_string_pretty(&serde_json::json!({"revision":TEST262_REVISION,"selected_files":paths.len(),"fixture_git_blobs":blobs,"AST":ast,"bytecode":bytecode,"outcomes":outcomes,"scope":"Original whole test programs compiled/executed independently in AST and VM, fresh Realms. Original sta/assert/includes execute as AST-backed host harness for both backends; no test-program AST fallback or assertion stripping. Unsupported/skipped are not pass."})).unwrap()).unwrap();
    }
    println!("AST: {ast}; bytecode: {bytecode}");
    assert!(
        outcomes.iter().all(|row| row["status"] == "pass"),
        "closure upstream integration remains incomplete"
    );
}
