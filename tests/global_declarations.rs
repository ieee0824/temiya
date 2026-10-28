use temiya::{
    Engine,
    runtime::{Completion, PropertyDescriptor, Realm, RuntimeErrorKind, ScriptRecord, Value},
};

fn script(realm: &Realm, source: &str) -> temiya::runtime::RuntimeResult<Completion> {
    ScriptRecord::parse(realm.clone(), source)?.evaluate()
}
fn normal(realm: &Realm, source: &str) -> Value {
    let Completion::Normal(value) = script(realm, source).unwrap() else {
        panic!("normal completion required")
    };
    value.unwrap_or(Value::Undefined)
}
fn own(realm: &Realm, name: &str) -> Option<PropertyDescriptor> {
    realm.global_object().get_own_property(&name.into())
}

#[test]
fn public_engine_exposes_global_var_and_function_on_this() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let execute = |source| {
        engine
            .evaluate(&engine.compile(&realm, source).unwrap())
            .unwrap()
            .value()
            .clone()
    };
    assert!(execute("var declared=1; this.declared;").same_value(&Value::Number(1.0)));
    assert!(execute("var declared; declared;").same_value(&Value::Number(1.0)));
    assert!(
        execute("function declared(){return 7;} this.declared();").same_value(&Value::Number(7.0))
    );
    assert!(execute("let lexical=2; const immutable=3; this.lexical===undefined && this.immutable===undefined && lexical+immutable===5;").same_value(&Value::Boolean(true)));
    assert!(
        execute("function duplicate(){return 1;} function duplicate(){return 2;} duplicate();")
            .same_value(&Value::Number(2.0))
    );
    let other = engine.create_realm();
    assert!(
        engine
            .evaluate(&engine.compile(&other, "this.declared;").unwrap())
            .unwrap()
            .value()
            .same_value(&Value::Undefined)
    );
}

#[test]
fn global_records_descriptors_and_cross_script_conflicts() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&allocation_context);
    normal(
        &realm,
        "var variable=4; function callable(){return 5;} let lexical=6; const fixed=7;",
    );
    for name in ["variable", "callable"] {
        let descriptor = own(&realm, name).unwrap();
        assert_eq!(
            (
                descriptor.writable,
                descriptor.enumerable,
                descriptor.configurable
            ),
            (Some(true), Some(true), Some(false))
        );
        assert!(
            realm
                .global_environment()
                .has_var_declaration(&name.into())
                .unwrap()
        );
    }
    for name in ["lexical", "fixed"] {
        assert!(own(&realm, name).is_none());
        assert!(
            realm
                .global_environment()
                .has_lexical_declaration(&name.into())
                .unwrap()
        );
    }
    for source in [
        "let variable;",
        "let callable;",
        "var lexical;",
        "function fixed(){}",
        "let lexical;",
    ] {
        assert_eq!(
            script(&realm, source).unwrap_err().kind(),
            RuntimeErrorKind::SyntaxError,
            "{source}"
        );
    }
    assert!(normal(&realm, "var variable; variable;").same_value(&Value::Number(4.0)));
}

#[test]
fn failed_preflight_does_not_create_other_bindings() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&allocation_context);
    normal(&realm, "let existing=1;");
    assert_eq!(
        script(
            &realm,
            "var innocent; let fresh=2; function alsoInnocent(){} function existing(){}"
        )
        .unwrap_err()
        .kind(),
        RuntimeErrorKind::SyntaxError
    );
    assert!(own(&realm, "innocent").is_none());
    assert!(own(&realm, "alsoInnocent").is_none());
    assert!(
        !realm
            .global_environment()
            .has_lexical_declaration(&"fresh".into())
            .unwrap()
    );
    realm.global_object().prevent_extensions();
    assert_eq!(
        script(&realm, "var denied; let existing;")
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::SyntaxError
    );
    assert_eq!(
        script(&realm, "let fresh; var denied;").unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert!(
        !realm
            .global_environment()
            .has_lexical_declaration(&"fresh".into())
            .unwrap()
    );
}

#[test]
fn restricted_existing_properties_and_strict_initializers() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&allocation_context);
    realm
        .global_object()
        .define_own_property(
            "locked".into(),
            PropertyDescriptor::data(Value::Number(1.0), false, true, false),
        )
        .unwrap();
    assert_eq!(
        script(&realm, "let locked;").unwrap_err().kind(),
        RuntimeErrorKind::SyntaxError
    );
    assert_eq!(
        script(&realm, "function locked(){}").unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert!(normal(&realm, "var locked=2; locked;").same_value(&Value::Number(1.0)));
    assert_eq!(
        script(&realm, "'use strict'; var locked=2;")
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
    assert!(normal(&realm, "locked;").same_value(&Value::Number(1.0)));
    realm
        .global_object()
        .define_own_property(
            "replaceable".into(),
            PropertyDescriptor::data(Value::Number(3.0), false, false, true),
        )
        .unwrap();
    normal(&realm, "function replaceable(){return 4;}");
    let descriptor = own(&realm, "replaceable").unwrap();
    assert_eq!(
        (
            descriptor.writable,
            descriptor.enumerable,
            descriptor.configurable
        ),
        (Some(true), Some(true), Some(false))
    );
    assert!(normal(&realm, "replaceable();").same_value(&Value::Number(4.0)));
}

#[test]
fn existing_configurable_property_reuse_allows_later_lexical_shadowing() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for extensible in [true, false] {
        let realm = Realm::ordinary_in(&allocation_context);
        realm
            .global_object()
            .define_own_property(
                "existing".into(),
                PropertyDescriptor::data(Value::Number(42.0), true, true, true),
            )
            .unwrap();
        if !extensible {
            realm.global_object().prevent_extensions();
        }
        assert!(normal(&realm, "var existing; existing;").same_value(&Value::Number(42.0)));
        assert!(normal(&realm, "let existing=2; existing;").same_value(&Value::Number(2.0)));
        assert!(
            realm
                .global_object()
                .get(&"existing".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
    }
}

#[test]
fn function_and_block_bindings_do_not_become_globals() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    for source in [
        "function run(p){var local=p; {let p=9; local=local+p;} return local;} run(3)===12 && this.local===undefined && this.p===undefined;",
        "{ let onlyBlock=3; function blockFunction(){return onlyBlock;} blockFunction(); } this.onlyBlock===undefined && this.blockFunction===undefined;",
        "function outer(){let captured=4; return function(){return captured;};} var closure=outer(); closure()===4;",
    ] {
        let value = engine
            .evaluate(&engine.compile(&realm, source).unwrap())
            .unwrap();
        assert!(
            value.value().same_value(&Value::Boolean(true)),
            "{source}: {value:?}"
        );
    }
}

#[test]
#[ignore = "requires pinned Test262 corpus in TEST262_DIR"]
fn pinned_test262_global_declarations() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::process::Command;
    use temiya::{
        runtime::{Object, RuntimeError},
        test262::{NegativePhase, TEST262_REVISION, TestCase, TestVariant},
    };
    let corpus = std::env::var("TEST262_DIR").expect("TEST262_DIR required");
    let read = |path: &str| {
        let output = Command::new("git")
            .args(["-C", &corpus, "show", &format!("{TEST262_REVISION}:{path}")])
            .output()
            .unwrap();
        assert!(output.status.success(), "missing pinned {path}");
        String::from_utf8(output.stdout).unwrap()
    };
    let selection = std::env::var("TEST262_CASE").unwrap_or_else(|_| {
        [
            "decl-func-dup.js",
            "decl-lex-configurable-global.js",
            "decl-lex-restricted-global.js",
            "script-decl-func-dups.js",
            "script-decl-var-err.js",
            "script-decl-func-err-non-configurable.js",
            "script-decl-func-err-non-extensible.js",
        ]
        .join(",")
    });
    let mut outcomes = vec![];
    let (mut passed, mut failed, mut skipped) = (0, 0, 0);
    for name in selection.split(',') {
        assert!(name.ends_with(".js") && !name.contains(['/', '\\']) && !name.starts_with('.'));
        let path = format!("test/language/global-code/{name}");
        let case = TestCase::parse(&path, read(&path)).unwrap();
        if case
            .metadata
            .flags
            .iter()
            .any(|flag| matches!(flag.as_str(), "module" | "async" | "raw"))
        {
            skipped += 1;
            outcomes.push(serde_json::json!({"path":path,"status":"skipped","reason":"unsupported metadata, not a pass"}));
            continue;
        }
        for variant in case.variants() {
            assert!(matches!(variant, TestVariant::Sloppy | TestVariant::Strict));
            let run = || -> Result<(), String> {
                let realm = Realm::ordinary_in(&allocation_context);
                // Actual ScriptRecord parse/evaluate in this same Realm, not a
                // stubbed result. This supplies only evalScript for this runner.
                let eval_realm = realm.clone();
                let host = Object::new_in(&allocation_context);
                host.create_data_property(
                    "evalScript".into(),
                    Value::Object(Object::native_function_in(
                        &allocation_context,
                        move |_, args| {
                            let source = args
                                .first()
                                .cloned()
                                .unwrap_or(Value::Undefined)
                                .to_js_string()?
                                .to_string_lossy();
                            match ScriptRecord::parse(eval_realm.clone(), &source)?.evaluate()? {
                                Completion::Normal(value) => Ok(value.unwrap_or(Value::Undefined)),
                                Completion::Throw(value) => Err(RuntimeError::thrown(value)),
                                _ => Err(RuntimeError::internal_error(
                                    "invalid completion from evalScript",
                                )),
                            }
                        },
                    )),
                )
                .map_err(|e| e.to_string())?;
                realm
                    .global_object()
                    .create_data_property("$262".into(), Value::Object(host))
                    .map_err(|e| e.to_string())?;
                let result = (|| {
                    for harness in ["sta.js", "assert.js"]
                        .into_iter()
                        .chain(case.metadata.includes.iter().map(String::as_str))
                    {
                        ScriptRecord::parse(realm.clone(), &read(&format!("harness/{harness}")))
                            .map_err(|e| format!("harness parse {harness}: {e}"))?
                            .evaluate()
                            .map_err(|e| format!("harness evaluate {harness}: {e:?}"))?;
                    }
                    let parsed = ScriptRecord::parse(realm.clone(), &case.source_for(variant));
                    match (&case.metadata.negative, parsed) {
                        (Some(expected), Err(error))
                            if expected.phase == NegativePhase::Parse
                                && expected.error_type == error.kind().to_string() =>
                        {
                            Ok(())
                        }
                        (_, Err(error)) => Err(format!("unexpected parse failure: {error:?}")),
                        (Some(expected), Ok(_)) if expected.phase == NegativePhase::Parse => {
                            Err("expected parse failure; source parsed".into())
                        }
                        (negative, Ok(script)) => match (negative, script.evaluate()) {
                            (None, Ok(Completion::Normal(_))) => Ok(()),
                            (Some(expected), Err(error))
                                if expected.phase == NegativePhase::Runtime
                                    && expected.error_type == error.kind().to_string() =>
                            {
                                Ok(())
                            }
                            (expected, observed) => {
                                Err(format!("expected {expected:?}, observed {observed:?}"))
                            }
                        },
                    }
                })();
                // Break the test host callback's strong Realm edge even if a
                // fixture prevents global extensions or evaluation fails.
                if !realm.global_object().delete(&"$262".into()) {
                    return Err("test host cleanup rejected".into());
                }
                result
            };
            match run() {
                Ok(()) => {
                    passed += 1;
                    println!("PASS {path} {variant:?}");
                    outcomes.push(serde_json::json!({"path":path,"variant":format!("{variant:?}"),"status":"pass"}));
                }
                Err(error) => {
                    failed += 1;
                    println!("FAIL {path} {variant:?}: {error}");
                    outcomes.push(serde_json::json!({"path":path,"variant":format!("{variant:?}"),"status":"fail","error":error}));
                }
            }
        }
    }
    if let Ok(path) = std::env::var("TEST262_REPORT") {
        std::fs::write(path,serde_json::to_string_pretty(&serde_json::json!({"revision":TEST262_REVISION,"selected_files":selection.split(',').count(),"passed":passed,"failed":failed,"skipped_files":skipped,"host":"evalScript callback uses real ScriptRecord parse/evaluate in same Realm; other $262 hooks absent","outcomes":outcomes})).unwrap()).unwrap();
    }
    println!(
        "Test262 {TEST262_REVISION}: {passed} passed; {failed} failed; {skipped} skipped files"
    );
    assert!(
        passed > 0 && failed == 0 && skipped == 0,
        "selection not fully passing"
    );
}

#[test]
fn global_state_and_errors_match_scoped_bytecode_api() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::bytecode::{BytecodeCompiler, VirtualMachine};
    let ast = Realm::ordinary_in(&allocation_context);
    let vm = Realm::ordinary_in(&allocation_context);
    for source in [
        "var count=1; let lexical=2; const fixed=3; count+lexical;",
        "count=4; {let count=5; count;} count+lexical;",
        "var count; count;",
        "var innocent; let fixed;",
        "fixed=9;",
        "let count;",
        "let Array; 42;",
    ] {
        let a = script(&ast, source);
        let b = VirtualMachine::execute_in_realm(&BytecodeCompiler::compile(source).unwrap(), &vm);
        match (a, b) {
            (Ok(Completion::Normal(a)), Ok(Completion::Normal(b))) => assert!(
                a.unwrap_or(Value::Undefined)
                    .same_value(&b.unwrap_or(Value::Undefined)),
                "{source}"
            ),
            (Err(a), Err(b)) => assert_eq!(a.kind(), b.kind(), "{source}"),
            (a, b) => panic!("{source}: {a:?} / {b:?}"),
        }
        for name in ["count", "lexical", "fixed", "innocent"] {
            let a = own(&ast, name);
            let b = own(&vm, name);
            assert_eq!(a.is_some(), b.is_some(), "{source}: {name}");
            if let (Some(a), Some(b)) = (a, b) {
                assert_eq!(
                    (a.writable, a.enumerable, a.configurable),
                    (b.writable, b.enumerable, b.configurable)
                );
                assert!(a.value.unwrap().same_value(&b.value.unwrap()));
            }
        }
    }
}

#[test]
fn module_and_single_statement_entrypoints_keep_correct_records() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::{
        parser::parse_script,
        runtime::{ModuleGraph, StatementEvaluator},
    };
    let realm = Realm::ordinary_in(&allocation_context);
    let graph = ModuleGraph::new(realm.clone(), |_| {
        Ok("export var moduleLocal=3; export function read(){return moduleLocal;}".into())
    });
    let module = graph.load("entry").unwrap();
    module.link().unwrap();
    module.evaluate().unwrap();
    assert!(own(&realm, "moduleLocal").is_none());
    assert!(own(&realm, "read").is_none());
    assert!(
        module
            .get_export(&"moduleLocal".into())
            .unwrap()
            .same_value(&Value::Number(3.0))
    );
    let evaluator = StatementEvaluator::new(realm.clone());
    for source in [
        "var single=4;",
        "let singleLexical=5;",
        "function singleFunction(){return single;}",
    ] {
        let program = parse_script(source).unwrap();
        evaluator.evaluate_statement(&program.body[0]).unwrap();
    }
    assert!(
        own(&realm, "single")
            .unwrap()
            .value
            .unwrap()
            .same_value(&Value::Number(4.0))
    );
    assert!(own(&realm, "singleLexical").is_none());
    assert!(normal(&realm, "singleFunction();").same_value(&Value::Number(4.0)));
}

#[test]
fn existing_accessor_var_does_not_read_or_redefine_property() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::{cell::RefCell, rc::Rc};
    use temiya::runtime::Object;
    let realm = Realm::ordinary_in(&allocation_context);
    let log = Rc::new(RefCell::new(vec![]));
    let get_log = log.clone();
    let set_log = log.clone();
    realm
        .global_object()
        .define_own_property(
            "accessor".into(),
            PropertyDescriptor::accessor(
                Some(Object::native_function_in(
                    &allocation_context,
                    move |_, _| {
                        get_log.borrow_mut().push("get".to_owned());
                        Ok(Value::Number(8.0))
                    },
                )),
                Some(Object::native_function_in(
                    &allocation_context,
                    move |_, args| {
                        set_log
                            .borrow_mut()
                            .push(format!("set:{}", args[0].to_number()?));
                        Ok(Value::Undefined)
                    },
                )),
                true,
                true,
            ),
        )
        .unwrap();
    normal(&realm, "var accessor;");
    assert!(log.borrow().is_empty());
    normal(&realm, "var accessor=9;");
    assert_eq!(*log.borrow(), ["set:9"]);
    assert!(own(&realm, "accessor").unwrap().is_accessor_descriptor());
    assert!(normal(&realm, "accessor;").same_value(&Value::Number(8.0)));
    normal(&realm, "function accessor(){return 7;}");
    assert_eq!(*log.borrow(), ["set:9", "get"]);
    assert!(normal(&realm, "accessor();").same_value(&Value::Number(7.0)));
}
