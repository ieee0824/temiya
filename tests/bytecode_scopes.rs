use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, RuntimeErrorKind, StatementEvaluator, Value},
};

#[test]
fn block_iteration_clone_preserves_the_supplied_realms_context() {
    use temiya::bytecode::{
        BindingKind, Bytecode, Constant, Instruction, ScopeBinding, ScopeDescriptor, ScopeKind,
    };
    use temiya::runtime::{AllocationContext, Object};
    let context = AllocationContext::new();
    let realm = Realm::ordinary_in(&context);
    let code = Bytecode {
        constants: vec![
            Constant::Scope(ScopeDescriptor {
                kind: ScopeKind::Global,
                strict: false,
                bindings: vec![],
                functions: vec![],
            }),
            Constant::Scope(ScopeDescriptor {
                kind: ScopeKind::Block,
                strict: false,
                bindings: vec![ScopeBinding {
                    name: "value".into(),
                    kind: BindingKind::Let,
                }],
                functions: vec![],
            }),
        ],
        instructions: vec![
            Instruction::EnterScope(0),
            Instruction::EnterScope(1),
            Instruction::CreateObject,
            Instruction::InitializeBinding { scope: 1, slot: 0 },
            Instruction::CloneLoopBindings,
            Instruction::LoadBinding { scope: 1, slot: 0 },
            Instruction::ExitScope,
            Instruction::ExitScope,
            Instruction::Complete,
        ],
        local_count: 0,
    }
    .verify()
    .unwrap();
    let Completion::Normal(Some(Value::Object(object))) =
        VirtualMachine::execute_in_realm(&code, &realm).unwrap()
    else {
        panic!("object result")
    };
    let peer = Object::new_in(&context);
    object
        .create_data_property("peer".into(), Value::Object(peer.clone()))
        .unwrap();
    context.collect_garbage().unwrap();
    assert!(
        object
            .get(&"peer".into())
            .unwrap()
            .same_value(&Value::Object(peer.clone()))
    );
    drop((object, peer, realm));
    assert_eq!(context.collect_garbage().unwrap().retained, 0);
}

fn compare(source: &str) {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let program = parse_script(source).unwrap();
    let ast =
        StatementEvaluator::new(Realm::ordinary_in(&allocation_context)).evaluate_program(&program);
    let code = BytecodeCompiler::compile_program(&program).unwrap();
    let vm = VirtualMachine::execute_in_realm(&code, &Realm::ordinary_in(&allocation_context));
    match (ast, vm) {
        (Ok(Completion::Normal(Some(a))), Ok(Completion::Normal(Some(b))))
        | (Ok(Completion::Throw(a)), Ok(Completion::Throw(b))) => {
            assert!(a.same_value(&b), "{source}: {a:?} / {b:?}")
        }
        (Err(a), Err(b)) => assert_eq!(a.kind(), b.kind(), "{source}"),
        (a, b) => panic!("{source}: {a:?} / {b:?}"),
    }
}

#[test]
fn shadowing_hoisting_and_loop_scope_match_ast() {
    for source in [
        "let x=1; { let x=2; x=x+3; } x;",
        "let x=1; { const x=4; x; }",
        "var x=7; { var x; } x;",
        "let n=0; while(!(n===2)) { let fresh=n; n=fresh+1; } n;",
        "x; var x=2; x;",
        "let n=0; let run=true; while(run) { let x=41; n=x+1; run=false; } n;",
        "if (true) { let x=1; } else { let x=2; } 42;",
        "{ let x; x; }",
        "let x=3; { let y=x+1; { let x=y+1; x; } }",
    ] {
        compare(source);
    }
}

#[test]
fn tdz_and_const_errors_match_ast() {
    for source in [
        "x; let x=1;",
        "let x=x+1;",
        "x=1; let x;",
        "let x=1; { x; let x=2; }",
        "const x=1; x=2;",
        "'use strict'; const x=1; x=2;",
        "{ const x=x; }",
        "{ let x=1; } x;",
    ] {
        compare(source);
    }
}

#[test]
fn persistent_global_records_and_preflight() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&allocation_context);
    let execute = |source| {
        VirtualMachine::execute_in_realm(&BytecodeCompiler::compile(source).unwrap(), &realm)
    };
    execute("var v=7; let l=8; const c=9;").unwrap();
    assert!(
        realm
            .global_object()
            .get(&"v".into())
            .unwrap()
            .same_value(&Value::Number(7.0))
    );
    assert!(
        realm
            .global_object()
            .get_own_property(&"l".into())
            .is_none()
    );
    assert!(
        realm
            .global_object()
            .get_own_property(&"c".into())
            .is_none()
    );
    let Completion::Normal(Some(value)) = execute("var v; v+l+c;").unwrap() else {
        panic!()
    };
    assert!(value.same_value(&Value::Number(24.0)));
    assert_eq!(
        execute("var innocent; let l=3;").unwrap_err().kind(),
        RuntimeErrorKind::SyntaxError
    );
    assert!(
        !realm
            .global_environment()
            .has_var_declaration(&"innocent".into())
            .unwrap()
    );
    assert_eq!(
        execute("c=10;").unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(
        execute("l; let l=1;").unwrap_err().kind(),
        RuntimeErrorKind::SyntaxError
    );
    execute("implicit=42;").unwrap();
    assert_eq!(
        execute("'use strict'; missing=1;").unwrap_err().kind(),
        RuntimeErrorKind::ReferenceError
    );
}

#[test]
fn errors_and_throw_discard_block_frames() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&allocation_context);
    for source in [
        "{let temporary=1; throw temporary;}",
        "{let temporary=temporary;}",
    ] {
        let _ =
            VirtualMachine::execute_in_realm(&BytecodeCompiler::compile(source).unwrap(), &realm);
        assert!(
            !realm
                .global_environment()
                .has_binding(&"temporary".into())
                .unwrap()
        );
    }
    let code = BytecodeCompiler::compile("42;").unwrap();
    assert!(matches!(
        VirtualMachine::execute_in_realm(&code, &realm),
        Ok(Completion::Normal(Some(Value::Number(42.0))))
    ));
}

#[test]
fn function_frame_var_block_and_outer_bindings_are_distinct() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::{
        ast::{FunctionBody, StmtKind},
        runtime::{Environment, ThisBindingStatus},
    };
    let realm = Realm::ordinary_in(&allocation_context);
    let program = parse_script(
        "var captured=3; function f(){ var x=4; { let x=5; captured=x; } return x+captured; }",
    )
    .unwrap();
    StatementEvaluator::new(realm.clone())
        .evaluate_program(&program)
        .unwrap();
    let StmtKind::FunctionDeclaration(function) = &program.body[1].kind else {
        panic!()
    };
    let Value::Object(object) = realm
        .global_environment()
        .get_identifier_value(&"f".into(), true)
        .unwrap()
    else {
        panic!()
    };
    let frame = Environment::function(
        Some(realm.global_environment()),
        object,
        None,
        ThisBindingStatus::Initialized,
    );
    let code = BytecodeCompiler::compile_function_body(function).unwrap();
    assert!(matches!(
        VirtualMachine::execute_in_function(&code, &realm, &frame).unwrap(),
        Completion::Return(Value::Number(9.0))
    ));
    assert!(!realm.global_environment().has_binding(&"x".into()).unwrap());
    assert!(
        frame
            .get_binding_value(&"x".into(), true)
            .unwrap()
            .same_value(&Value::Number(4.0))
    );
    assert_eq!(
        VirtualMachine::execute_in_realm(&code, &realm)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(
        VirtualMachine::execute_in_function(&code, &realm, &realm.global_environment())
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
    let mut fallthrough = function.as_ref().clone();
    let body = parse_script("{ let x=1; x; }").unwrap();
    fallthrough.body = FunctionBody::Block(Box::new(body.body[0].clone()));
    let code = BytecodeCompiler::compile_function_body(&fallthrough).unwrap();
    assert!(matches!(
        VirtualMachine::execute_in_function(&code, &realm, &frame).unwrap(),
        Completion::Return(Value::Undefined)
    ));
}

#[test]
fn caller_prepared_scalar_loop_frames_publish_their_binding_writes() {
    use temiya::{
        ast::StmtKind,
        runtime::{Environment, ThisBindingStatus},
    };
    let realm = Realm::ordinary();
    let program =
        parse_script("function f(){var n=3;var sum=0;for(var i=0;i<n;i++){sum+=i;}return sum;}")
            .unwrap();
    let StmtKind::FunctionDeclaration(function) = &program.body[0].kind else {
        panic!("function")
    };
    let object = temiya::runtime::Object::new_in(realm.allocation_context());
    let frame = Environment::function(
        Some(realm.global_environment()),
        object,
        None,
        ThisBindingStatus::Initialized,
    );
    let code = BytecodeCompiler::compile_function_body(function).unwrap();
    assert!(matches!(
        VirtualMachine::execute_in_function(&code, &realm, &frame).unwrap(),
        Completion::Return(Value::Number(3.0))
    ));
    for name in ["sum", "i", "n"] {
        assert!(
            frame
                .get_binding_value(&name.into(), true)
                .unwrap()
                .same_value(&Value::Number(3.0)),
            "{name}"
        );
    }
}

#[test]
fn scoped_addition_calls_real_javascript_coercion() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&allocation_context);
    StatementEvaluator::new(realm.clone()).evaluate_program(&parse_script("var order=''; var a={valueOf:function(){order=order+'L';return 'a';}}; var b={valueOf:function(){order=order+'R';return 2;}};").unwrap()).unwrap();
    let code = BytecodeCompiler::compile("a+b;").unwrap();
    let Completion::Normal(Some(value)) = VirtualMachine::execute_in_realm(&code, &realm).unwrap()
    else {
        panic!()
    };
    assert!(value.same_value(&Value::String("a2".into())));
    assert!(
        realm
            .global_environment()
            .get_identifier_value(&"order".into(), true)
            .unwrap()
            .same_value(&Value::String("LR".into()))
    );
}

#[test]
#[ignore = "requires scripts/test262 pinned corpus"]
fn pinned_test262_bytecode_scope_tdz() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::process::Command;
    use temiya::test262::{NegativePhase, TEST262_REVISION, TestCase, TestVariant};
    let root = std::env::var("TEST262_DIR").expect("TEST262_DIR required");
    let revision = Command::new("git")
        .args(["-C", &root, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(revision.status.success());
    assert_eq!(
        String::from_utf8(revision.stdout).unwrap().trim(),
        TEST262_REVISION
    );
    let read_pinned = |path: &str| {
        let output = Command::new("git")
            .args(["-C", &root, "show", &format!("{TEST262_REVISION}:{path}")])
            .output()
            .unwrap();
        assert!(output.status.success(), "missing pinned {path}");
        String::from_utf8(output.stdout).unwrap()
    };
    let mut count = 0;
    for kind in ["let", "const"] {
        for suffix in ["prior-statement", "declaration-statement"] {
            let path = format!(
                "test/language/statements/{kind}/global-use-before-initialization-in-{suffix}.js"
            );
            let source = read_pinned(&path);
            let test = TestCase::parse(&path, source).unwrap();
            let negative = test.metadata.negative.as_ref().unwrap();
            assert_eq!(negative.phase, NegativePhase::Runtime);
            assert_eq!(negative.error_type, "ReferenceError");
            assert!(test.metadata.includes.is_empty());
            for variant in test.variants() {
                assert!(matches!(variant, TestVariant::Sloppy | TestVariant::Strict));
                let source = if variant == TestVariant::Strict {
                    format!("\"use strict\";\n{}", test.source)
                } else {
                    test.source.clone()
                };
                let program = parse_script(&source).unwrap();
                // Runtime-negative tests must successfully compile before execution.
                let code = BytecodeCompiler::compile_program(&program).unwrap();
                for (is_vm, realm) in [
                    (true, Realm::ordinary_in(&allocation_context)),
                    (false, Realm::ordinary_in(&allocation_context)),
                ] {
                    for harness in ["sta.js", "assert.js"] {
                        let source = read_pinned(&format!("harness/{harness}"));
                        StatementEvaluator::new(realm.clone())
                            .evaluate_program(&parse_script(&source).unwrap())
                            .unwrap();
                    }
                    let result = if is_vm {
                        VirtualMachine::execute_in_realm(&code, &realm)
                    } else {
                        StatementEvaluator::new(realm).evaluate_program(&program)
                    };
                    assert_eq!(
                        result.unwrap_err().kind(),
                        RuntimeErrorKind::ReferenceError,
                        "{path} {variant:?}"
                    );
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 16); // Eight original variants, each executed by AST and VM.
    println!("8 pinned runtime-negative variants passed in both AST and bytecode; 0 skipped");
}

#[test]
fn dynamic_assignment_resolves_before_rhs_coercion_changes_environment() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::runtime::Object;
    for is_vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        let environment = realm.global_environment();
        let callback_environment = environment.clone();
        let trigger = Object::new_in(&allocation_context);
        trigger.set_to_primitive_hook(move |_| {
            callback_environment.create_global_lexical_binding("target".into(), true, false)?;
            callback_environment.initialize_binding(&"target".into(), Value::Number(99.0))?;
            Ok(Value::Number(42.0))
        });
        environment
            .create_mutable_binding("trigger".into(), true)
            .unwrap();
        environment
            .initialize_binding(&"trigger".into(), Value::Object(trigger))
            .unwrap();
        let source = "target=trigger+0;";
        if is_vm {
            VirtualMachine::execute_in_realm(&BytecodeCompiler::compile(source).unwrap(), &realm)
                .unwrap();
        } else {
            StatementEvaluator::new(realm.clone())
                .evaluate_program(&parse_script(source).unwrap())
                .unwrap();
        }
        assert!(
            environment
                .get_identifier_value(&"target".into(), true)
                .unwrap()
                .same_value(&Value::Number(99.0))
        );
        assert!(
            realm
                .global_object()
                .get(&"target".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        environment.delete_binding(&"trigger".into()).unwrap();
    }
}

#[test]
fn global_restrictions_are_checked_before_binding_creation() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::runtime::PropertyDescriptor;
    let realm = Realm::ordinary_in(&allocation_context);
    realm
        .global_object()
        .define_own_property(
            "fixed".into(),
            PropertyDescriptor::data(Value::Number(1.0), false, true, false),
        )
        .unwrap();
    let execute = |source| {
        VirtualMachine::execute_in_realm(&BytecodeCompiler::compile(source).unwrap(), &realm)
    };
    assert_eq!(
        execute("let fixed=2;").unwrap_err().kind(),
        RuntimeErrorKind::SyntaxError
    );
    assert_eq!(
        execute("'use strict'; fixed=2;").unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    execute("let existing=1;").unwrap();
    realm.global_object().prevent_extensions();
    assert_eq!(
        execute("var denied; let existing=2;").unwrap_err().kind(),
        RuntimeErrorKind::SyntaxError
    );
    assert_eq!(
        execute("var denied; let innocent=2;").unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert!(
        !realm
            .global_environment()
            .has_lexical_declaration(&"innocent".into())
            .unwrap()
    );
}

#[test]
fn function_body_tdz_const_and_hoisting_match_real_ast_calls() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::{
        ast::StmtKind,
        runtime::{Environment, ThisBindingStatus},
    };
    for source in [
        "function f(){ let x=x; }",
        "function f(){ const x=1; x=2; }",
        "function f(){ { const x=2; return x; } }",
        "function f(){ return x; var x=1; }",
    ] {
        let realm = Realm::ordinary_in(&allocation_context);
        let program = parse_script(source).unwrap();
        StatementEvaluator::new(realm.clone())
            .evaluate_program(&program)
            .unwrap();
        let ast =
            StatementEvaluator::new(realm.clone()).evaluate_program(&parse_script("f();").unwrap());
        let StmtKind::FunctionDeclaration(function) = &program.body[0].kind else {
            panic!()
        };
        let Value::Object(object) = realm
            .global_environment()
            .get_identifier_value(&"f".into(), true)
            .unwrap()
        else {
            panic!()
        };
        let frame = Environment::function(
            Some(realm.global_environment()),
            object,
            None,
            ThisBindingStatus::Initialized,
        );
        let vm = VirtualMachine::execute_in_function(
            &BytecodeCompiler::compile_function_body(function).unwrap(),
            &realm,
            &frame,
        );
        match (ast, vm) {
            (Err(a), Err(b)) => assert_eq!(a.kind(), b.kind(), "{source}"),
            (Ok(Completion::Normal(Some(a))), Ok(Completion::Return(b))) => {
                assert!(a.same_value(&b), "{source}")
            }
            (a, b) => panic!("{source}: {a:?} / {b:?}"),
        }
    }
}
