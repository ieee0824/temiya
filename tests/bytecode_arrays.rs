use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, RuntimeErrorKind, RuntimeResult, StatementEvaluator, Value},
};
fn execute(realm: &Realm, source: &str, vm: bool) -> RuntimeResult<Completion> {
    if vm {
        VirtualMachine::execute_in_realm(&BytecodeCompiler::compile(source).unwrap(), realm)
    } else {
        StatementEvaluator::new(realm.clone()).evaluate_program(&parse_script(source).unwrap())
    }
}
fn check(realm: &Realm, source: &str, vm: bool) {
    assert!(
        matches!(
            execute(realm, source, vm).unwrap(),
            Completion::Normal(Some(Value::Boolean(true)))
        ),
        "{source}"
    );
}
#[test]
fn holes_empty_spreads_and_trailing_commas_preserve_length_and_own_properties() {
    for vm in [false, true] {
        for source in [
            "var a=[,,42,,];a.length===4&&!(0 in a)&&!(1 in a)&&a[2]===42&&!(3 in a);",
            "var a=[...[]];var b=[...[],];var c=[,...[],];a.length===0&&b.length===0&&c.length===1&&!(0 in c);",
            "var a=[...[,,]];a.length===2&&Object.prototype.hasOwnProperty.call(a,'0')&&a[0]===undefined&&Object.prototype.hasOwnProperty.call(a,'1');",
            "var a=[...'\u{1f600}'];a.length===1&&a[0].length===2;",
            "var a=[1,[2,,3],4];a.length===3&&a[1].length===3&&!(1 in a[1]);",
        ] {
            check(&Realm::ordinary(), source, vm);
        }
    }
}
#[test]
fn custom_iterator_captures_next_once_and_preserves_evaluation_order() {
    for vm in [false, true] {
        let realm = Realm::ordinary();
        execute(&realm,"var log='';var n=0;function mark(x){log+=x;return x;}var iterator={next:function(){log+='N';iterator.next=function(){throw 99;};n++;return {done:n>2,value:n};},return:function(){log+='R';return {};}};var iterable={[Symbol.iterator]:function(){log+='I';return iterator;}};",false).unwrap();
        check(
            &realm,
            "var a=[mark(0),...iterable,mark(9)];a.length===4&&a[0]===0&&a[1]===1&&a[2]===2&&a[3]===9&&log==='0INNN9';",
            vm,
        );
    }
}
#[test]
fn iterator_failures_propagate_and_do_not_close_or_evaluate_later_elements() {
    for failing in ["next-get", "next-call", "done", "value"] {
        for vm in [false, true] {
            let realm = Realm::ordinary();
            let setup = format!(
                "var log='';var marker={{}};var failing='{failing}';var iterator={{get next(){{log+='G';if(failing==='next-get')throw marker;return function(){{log+='N';if(failing==='next-call')throw marker;return {{get done(){{log+='D';if(failing==='done')throw marker;return false;}},get value(){{log+='V';throw marker;}}}};}};}},return:function(){{log+='R';return {{}};}}}};var iterable={{[Symbol.iterator]:function(){{log+='I';return iterator;}}}};function later(){{log+='L';return 0;}}"
            );
            execute(&realm, &setup, false).unwrap();
            let err = execute(&realm, "[...iterable,later()];", vm).unwrap_err();
            assert!(
                err.thrown_value()
                    .unwrap()
                    .same_value(&realm.global_object().get(&"marker".into()).unwrap())
            );
            let expected = match failing {
                "next-get" => "IG",
                "next-call" => "IGN",
                "done" => "IGND",
                _ => "IGNDV",
            };
            check(&realm, &format!("log==='{expected}';"), false);
        }
    }
}
#[test]
fn array_like_objects_require_an_iterator() {
    for vm in [false, true] {
        let realm = Realm::ordinary();
        execute(&realm, "var value={0:42,length:1};", false).unwrap();
        assert_eq!(
            execute(&realm, "[...value];", vm).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
    }
}

#[test]
fn array_instructions_check_scope_stack_state_and_length_overflow() {
    use temiya::bytecode::{
        Bytecode, Constant, Instruction, ScopeDescriptor, ScopeKind, VerifyErrorKind,
    };
    let scope = Constant::Scope(ScopeDescriptor {
        kind: ScopeKind::Global,
        strict: false,
        bindings: vec![],
        functions: vec![],
    });
    let code = Bytecode {
        constants: vec![],
        instructions: vec![Instruction::CreateArray, Instruction::Return],
        local_count: 0,
    };
    assert_eq!(
        code.verify().unwrap_err().kind(),
        VerifyErrorKind::ScopeMismatch
    );
    for op in [
        Instruction::AppendArray,
        Instruction::SpreadArray,
        Instruction::ElideArray,
        Instruction::FinishArray,
    ] {
        let code = Bytecode {
            constants: vec![scope.clone()],
            instructions: vec![Instruction::EnterScope(0), op, Instruction::Return],
            local_count: 0,
        };
        assert_eq!(
            code.verify().unwrap_err().kind(),
            VerifyErrorKind::StackUnderflow
        );
    }
    let code = Bytecode {
        constants: vec![scope, Constant::Number(f64::from(u32::MAX))],
        instructions: vec![
            Instruction::EnterScope(0),
            Instruction::CreateArray,
            Instruction::Pop,
            Instruction::LoadConstant(1),
            Instruction::ElideArray,
            Instruction::FinishArray,
            Instruction::Return,
        ],
        local_count: 0,
    }
    .verify()
    .unwrap();
    assert_eq!(
        VirtualMachine::execute_in_realm(&code, &Realm::ordinary())
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::RangeError
    );
}

#[test]
#[ignore = "requires pinned Test262 corpus in TEST262_DIR"]
fn pinned_test262_array_literals() {
    use std::process::Command;
    use temiya::test262::{NegativePhase, TEST262_REVISION, TestCase};
    let root = std::env::var("TEST262_DIR").expect("TEST262_DIR required");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(["-c", &format!("safe.directory={root}"), "-C", &root])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(git(&["rev-parse", "HEAD"]).trim(), TEST262_REVISION);
    let read = |path: &str| git(&["show", &format!("{TEST262_REVISION}:{path}")]);
    let mut outcomes = Vec::new();
    for name in [
        "S11.1.4_A1.1.js",
        "S11.1.4_A1.2.js",
        "S11.1.4_A1.3.js",
        "spread-sngl-empty.js",
        "spread-sngl-literal.js",
        "spread-sngl-expr.js",
    ] {
        let path = format!("test/language/expressions/array/{name}");
        let case = TestCase::parse(&path, read(&path)).unwrap();
        let blob = git(&["rev-parse", &format!("{TEST262_REVISION}:{path}")]);
        for variant in case.variants() {
            for vm in [false, true] {
                let source = case.source_for(variant);
                let run = || -> Result<(), String> {
                    if let Some(negative) = &case.metadata.negative {
                        assert_eq!(negative.phase, NegativePhase::Parse);
                        assert_eq!(negative.error_type, "SyntaxError");
                        return if parse_script(&source).is_err()
                            && BytecodeCompiler::compile(&source).is_err()
                        {
                            Ok(())
                        } else {
                            Err("parse-negative accepted".into())
                        };
                    }
                    let realm = Realm::ordinary();
                    for harness in ["sta.js", "assert.js"]
                        .into_iter()
                        .chain(case.metadata.includes.iter().map(String::as_str))
                    {
                        match execute(&realm, &read(&format!("harness/{harness}")), false)
                            .map_err(|e| format!("harness: {e:?}"))?
                        {
                            Completion::Normal(_) => (),
                            other => return Err(format!("harness {other:?}")),
                        }
                    }
                    match execute(&realm, &source, vm).map_err(|e| format!("runtime: {e:?}"))? {
                        Completion::Normal(_) => Ok(()),
                        other => Err(format!("completion: {other:?}")),
                    }
                };
                let result = run();
                println!("{path} {variant:?} vm={vm} {result:?}");
                outcomes.push(serde_json::json!({"path":path,"blob":blob.trim(),"variant":format!("{variant:?}"),"backend":if vm{"VM"}else{"AST"},"status":if result.is_ok(){"pass"}else{"fail"},"message":result.err()}));
            }
        }
    }
    if let Ok(path) = std::env::var("TEST262_REPORT") {
        std::fs::write(
            path,
            serde_json::to_string_pretty(
                &serde_json::json!({"revision":TEST262_REVISION,"outcomes":outcomes}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    assert!(outcomes.iter().all(|x| x["status"] == "pass"));
}
