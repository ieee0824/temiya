use temiya::{
    bytecode::{
        BindingKind, Bytecode, BytecodeCompiler, Constant, Instruction, ScopeBinding,
        ScopeDescriptor, ScopeKind, VerifyErrorKind, VirtualMachine,
    },
    runtime::{Completion, Realm, RuntimeErrorKind, Value},
};

fn run(source: &str) -> Value {
    let code = BytecodeCompiler::compile(source).unwrap_or_else(|e| panic!("{source}: {e}"));
    let result = VirtualMachine::execute_in_realm(&code, &Realm::ordinary())
        .unwrap_or_else(|e| panic!("{source}: {e}"));
    match result {
        Completion::Normal(value) => value.unwrap_or(Value::Undefined),
        other => panic!("{other:?}"),
    }
}

#[test]
fn loops_labels_and_scope_unwind_follow_the_correct_targets() {
    for (source, expected) in [
        (
            "var s=0;for(var i=0;i<5;i++){if(i===2)continue;if(i===4)break;s+=i;}s;",
            4.0,
        ),
        ("var n=0;do{n++;if(n<3)continue;}while(n<3);n;", 3.0),
        ("var n=0;while(true){n++;if(n===3)break;}n;", 3.0),
        (
            "var s=0;outer:for(let i=0;i<3;i++){let x=i;for(let j=0;j<3;j++){let y=j;if(j===1)continue outer;s+=x+y;}}s;",
            3.0,
        ),
        (
            "var s=0;a:b:for(let i=0;i<4;i++){let x=i;if(i===2)break a;s+=x;}s;",
            1.0,
        ),
        ("var s=1;done:{let x=41;s+=x;break done;s=0;}s;", 42.0),
        (
            "var i=0;while(i<3){i++;exit:{let x=99;if(i<3)continue;break exit;}break;}i;",
            3.0,
        ),
        ("var n=0;for(;;){n++;if(n===2)break;}n;", 2.0),
        (
            "var n=0;for(var i=0;i<4;i++){n++;for(var j=0;j<4;j++){break;}}n;",
            4.0,
        ),
    ] {
        assert!(run(source).same_value(&Value::Number(expected)), "{source}");
    }
}

#[test]
fn let_iteration_bindings_capture_initial_body_and_update_environments() {
    for source in [
        "var a,b;for(var i=0;i<2;i++){if(i===0)a=()=>i;else b=()=>i;}a()===2&&b()===2;",
        "var a,b,c;for(let i=0;i<3;i++){if(i===0)a=()=>i;if(i===1)b=()=>i;if(i===2)c=()=>i;}a()===0&&b()===1&&c()===2;",
        "var initial,body;for(let i=(initial=()=>i,0);i<2;i++){body=()=>i;}initial()===0&&body()===1;",
        "var a,b;for(let i=0,j=()=>i;i<2;i++){if(i===0)a=()=>j();else b=()=>j();}a()===0&&b()===0;",
        "var a,b;for(let i=0;i<2;i++){let x=i;let f=()=>x;if(i===0){a=f;continue;}b=f;}a()===0&&b()===1;",
        "var a,b;var n=0;for(const x=42;n<2;n++){if(n===0)a=()=>x;else b=()=>x;}a()===42&&b()===42;",
        "var i=42;for(let i=0;i<2;i++){}i===42;",
    ] {
        assert!(run(source).same_value(&Value::Boolean(true)), "{source}");
    }
}

#[test]
fn loop_completion_values_and_terminal_bodies_are_preserved() {
    for source in [
        "42;for(;false;){}",
        "42;while(false){}",
        "42;do{}while(false);",
        "42;if(false)99;",
        "while(true){42;if(true)break;}",
        "outer:while(true){42;while(true){break outer;}}",
    ] {
        assert!(run(source).same_value(&Value::Undefined), "{source}");
    }
    for source in [
        "for(;;){42;break;}",
        "do{42;break;}while(true);",
        "var n=0;do{n++;42;continue;}while(n<1);",
        "42;label:{break label;}",
        "(function(){for(;;){return 42;}})();",
        "(function(){do{return 42;}while(true);})();",
        "(function(){while(true){return 42;}})();",
    ] {
        assert!(run(source).same_value(&Value::Number(42.0)), "{source}");
    }
    assert!(run("(function(){for(var i=0;i<2;i++){42;}})();").same_value(&Value::Undefined));
}

#[test]
fn lexical_head_tdz_const_assignment_and_abrupt_errors_are_not_retried() {
    for (source, kind) in [
        (
            "let i=42;for(let i=i;false;){}",
            RuntimeErrorKind::ReferenceError,
        ),
        ("for(const i=0;i<1;i++){}", RuntimeErrorKind::TypeError),
    ] {
        let code = BytecodeCompiler::compile(source).unwrap();
        let error = VirtualMachine::execute_in_realm(&code, &Realm::ordinary()).unwrap_err();
        assert_eq!(error.kind(), kind);
    }
    let code = BytecodeCompiler::compile("var seen=0;for(;;){seen++;throw 42;}").unwrap();
    let realm = Realm::ordinary();
    assert!(matches!(
        VirtualMachine::execute_in_realm(&code, &realm).unwrap(),
        Completion::Throw(Value::Number(42.0))
    ));
    assert!(
        realm
            .global_object()
            .get(&"seen".into())
            .unwrap()
            .same_value(&Value::Number(1.0))
    );
}

#[test]
fn malformed_iteration_clone_cannot_copy_global_or_const_bindings() {
    let scope = |kind, bindings| {
        Constant::Scope(ScopeDescriptor {
            kind,
            strict: false,
            bindings,
            functions: vec![],
        })
    };
    let mut code = Bytecode {
        constants: vec![scope(ScopeKind::Global, vec![]), Constant::Number(42.0)],
        instructions: vec![
            Instruction::EnterScope(0),
            Instruction::LoadConstant(1),
            Instruction::ExitScope,
            Instruction::Complete,
        ],
        local_count: 0,
    };
    code.clone().verify().unwrap();
    code.instructions.insert(1, Instruction::CloneLoopBindings);
    assert_eq!(
        code.verify().unwrap_err().kind(),
        VerifyErrorKind::ScopeMismatch
    );
    let mut code = Bytecode {
        constants: vec![
            scope(ScopeKind::Global, vec![]),
            scope(
                ScopeKind::Block,
                vec![ScopeBinding {
                    name: "x".into(),
                    kind: BindingKind::Const,
                }],
            ),
            Constant::Number(42.0),
        ],
        instructions: vec![
            Instruction::EnterScope(0),
            Instruction::EnterScope(1),
            Instruction::LoadConstant(2),
            Instruction::InitializeBinding { scope: 1, slot: 0 },
            Instruction::LoadBinding { scope: 1, slot: 0 },
            Instruction::ExitScope,
            Instruction::ExitScope,
            Instruction::Complete,
        ],
        local_count: 0,
    };
    code.clone().verify().unwrap();
    code.instructions.insert(4, Instruction::CloneLoopBindings);
    assert_eq!(
        code.verify().unwrap_err().kind(),
        VerifyErrorKind::ScopeMismatch
    );
}

#[test]
fn invalid_jump_targets_are_rejected_even_in_unreachable_owned_ast() {
    for source in [
        "break;",
        "continue;",
        "label:{continue label;}",
        "a:a:while(false){}",
        "for(let i=0;i<1;i++){var i;}",
    ] {
        assert!(BytecodeCompiler::compile(source).is_err(), "{source}");
    }
    let mut program = temiya::parser::parse_script("throw 42;").unwrap();
    program.body.push(temiya::ast::Stmt {
        span: program.span,
        kind: temiya::ast::StmtKind::Break(Some("absent".into())),
    });
    assert!(BytecodeCompiler::compile_program(&program).is_err());
}

#[test]
#[ignore = "requires pinned Test262 corpus in TEST262_DIR"]
fn pinned_test262_bytecode_loops() {
    use std::{collections::BTreeMap, process::Command};
    use temiya::{
        parser::parse_script,
        runtime::ScriptRecord,
        test262::{NegativePhase, TEST262_REVISION, TestCase, TestVariant},
    };
    let root = std::env::var("TEST262_DIR").expect("TEST262_DIR required");
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-c")
            .arg(format!("safe.directory={root}"))
            .arg("-C")
            .arg(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    assert_eq!(git(&["rev-parse", "HEAD"]).trim(), TEST262_REVISION);
    let read = |path: &str| git(&["show", &format!("{TEST262_REVISION}:{path}")]);
    let paths = [
        "for/head-let-fresh-binding-per-iteration.js",
        "for/head-const-fresh-binding-per-iteration.js",
        "for/scope-head-lex-open.js",
        "for/scope-head-lex-close.js",
        "for/scope-head-var-none.js",
        "for/scope-body-lex-open.js",
        "for/scope-body-lex-boundary.js",
        "for/scope-body-var-none.js",
        "continue/nested-let-bound-for-loops-inner-continue.js",
        "continue/nested-let-bound-for-loops-outer-continue.js",
        "continue/nested-let-bound-for-loops-labeled-continue.js",
        "continue/shadowing-loop-variable-in-same-scope-as-continue.js",
        "continue/no-label-continue.js",
        "continue/labeled-continue.js",
        "continue/simple-and-labeled.js",
        "do-while/S12.6.1_A1.js",
        "do-while/S12.6.1_A2.js",
        "do-while/S12.6.1_A3.js",
        "do-while/S12.6.1_A4_T4.js",
        "while/S12.6.2_A1.js",
        "while/S12.6.2_A2.js",
        "while/S12.6.2_A3.js",
        "break/S12.8_A3.js",
        "labeled/continue.js",
    ];
    let mut blobs = BTreeMap::new();
    let mut outcomes = vec![];
    for suffix in paths {
        let path = format!("test/language/statements/{suffix}");
        blobs.insert(
            path.clone(),
            git(&["rev-parse", &format!("{TEST262_REVISION}:{path}")])
                .trim()
                .to_string(),
        );
        let case = TestCase::parse(&path, read(&path)).unwrap();
        for harness in ["sta.js", "assert.js"]
            .into_iter()
            .chain(case.metadata.includes.iter().map(String::as_str))
        {
            let p = format!("harness/{harness}");
            blobs.insert(
                p.clone(),
                git(&["rev-parse", &format!("{TEST262_REVISION}:{p}")])
                    .trim()
                    .to_string(),
            );
        }
        for variant in case.variants() {
            assert!(matches!(variant, TestVariant::Sloppy | TestVariant::Strict));
            let source = if variant == TestVariant::Strict {
                format!("'use strict';\n{}", case.source)
            } else {
                case.source.clone()
            };
            let run = || -> Result<(), String> {
                if let Some(negative) = &case.metadata.negative {
                    assert_eq!(negative.phase, NegativePhase::Parse);
                    assert_eq!(negative.error_type, "SyntaxError");
                    if parse_script(&source).is_ok() || BytecodeCompiler::compile(&source).is_ok() {
                        return Err("expected original parse-negative SyntaxError".into());
                    }
                    return Ok(());
                }
                let realm = Realm::ordinary();
                for harness in ["sta.js", "assert.js"]
                    .into_iter()
                    .chain(case.metadata.includes.iter().map(String::as_str))
                {
                    ScriptRecord::parse(realm.clone(), &read(&format!("harness/{harness}")))
                        .map_err(|e| format!("harness parse: {e}"))?
                        .evaluate()
                        .map_err(|e| format!("harness evaluation: {e}"))?;
                }
                let code =
                    BytecodeCompiler::compile(&source).map_err(|e| format!("compile: {e}"))?;
                match VirtualMachine::execute_in_realm(&code, &realm)
                    .map_err(|e| format!("runtime: {e:?}"))?
                {
                    Completion::Normal(_) => Ok(()),
                    other => Err(format!("unexpected completion: {other:?}")),
                }
            };
            let (status, message) = match run() {
                Ok(()) => ("pass", String::new()),
                Err(e) => ("fail", e),
            };
            println!("{status} {path} {variant:?} {message}");
            outcomes.push(serde_json::json!({"path":path,"variant":format!("{variant:?}"),"status":status,"message":message}));
        }
    }
    let passed = outcomes.iter().filter(|x| x["status"] == "pass").count();
    let report = serde_json::json!({"revision":TEST262_REVISION,"selected_files":paths.len(),"fixture_git_blobs":blobs,"passed":passed,"failed":outcomes.len()-passed,"skipped":0,"outcomes":outcomes,"scope":"Whole original selected loop/label programs. Original sta/assert/includes evaluated as AST host harness; every positive test program compiled and executed by explicit VM, no fallback or stripping. Original parse-negative programs rejected before execution. Does not establish AST/Engine or whole-loop-directory conformance."});
    if let Ok(path) = std::env::var("TEST262_REPORT") {
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
    assert_eq!(
        passed,
        outcomes.len(),
        "selected original loop gate remains incomplete"
    );
}
