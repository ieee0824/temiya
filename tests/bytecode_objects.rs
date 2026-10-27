use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, RuntimeResult, StatementEvaluator, Value},
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
fn data_keys_duplicates_shorthands_and_prototype_setters_are_distinct() {
    for vm in [false, true] {
        check(
            &Realm::ordinary(),
            "var n=2;var o={a:1,a:42,n,[1+1]:3,0x10:4,__proto__:null};o.a===42&&o.n===2&&o[2]===3&&o[16]===4&&Object.getPrototypeOf(o)===null&&!Object.prototype.hasOwnProperty.call(o,'__proto__');",
            vm,
        );
        check(
            &Realm::ordinary(),
            "var __proto__=42;var a={__proto__};var b={['__proto__']:7};var c={__proto__:3};a.__proto__===42&&b.__proto__===7&&Object.getPrototypeOf(c)===Object.prototype&&!Object.prototype.hasOwnProperty.call(c,'__proto__');",
            vm,
        );
    }
}
#[test]
fn computed_key_conversion_precedes_value_evaluation_once() {
    for vm in [false, true] {
        let realm = Realm::ordinary();
        execute(
            &realm,
            "var log='';var key={ [Symbol.toPrimitive](hint){log+=hint;return 'x';} };",
            false,
        )
        .unwrap();
        check(
            &realm,
            "var o={ [key]:(log+='V',42) };o.x===42&&log==='stringV';",
            vm,
        );
    }
}
#[test]
fn spread_copies_enumerable_values_symbols_and_utf16_without_setters() {
    for vm in [false, true] {
        let realm = Realm::ordinary();
        execute(&realm, "var hits=0;var sym=Symbol('x');var src={ get x(){hits++;return 42;} };src[sym]=7;Object.defineProperty(src,'hidden',{value:9,enumerable:false});", false).unwrap();
        check(
            &realm,
            "var o={...null,...undefined,...src,...'ab'};var d=Object.getOwnPropertyDescriptor(o,'x');o.x===42&&hits===1&&o[sym]===7&&!('hidden' in o)&&o[0]==='a'&&o[1]==='b'&&d.writable&&d.enumerable&&d.configurable&&!('get' in d);",
            vm,
        );
    }
}
#[test]
fn spread_propagates_own_keys_descriptor_and_getter_errors() {
    for trap in ["ownKeys", "getOwnPropertyDescriptor", "get"] {
        for vm in [false, true] {
            let realm = Realm::ordinary();
            let setup = format!(
                "var log='';var marker={{}};var target={{x:42}};var handler={{}};handler.{trap}=function(){{log+='T';throw marker;}};var src=new Proxy(target,handler);function hit(){{log+='H';return 0;}}"
            );
            execute(&realm, &setup, false).unwrap();
            let error = execute(&realm, "({before:hit(),...src,after:hit()});", vm).unwrap_err();
            let marker = realm.global_object().get(&"marker".into()).unwrap();
            assert!(error.thrown_value().unwrap().same_value(&marker));
            check(&realm, "log==='HT';", false);
        }
    }
}

#[test]
#[ignore = "requires pinned Test262 corpus in TEST262_DIR"]
fn pinned_test262_object_prototypes() {
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
        "__proto__-value-obj.js",
        "__proto__-value-null.js",
        "__proto__-value-non-object.js",
        "__proto__-duplicate.js",
        "__proto__-permitted-dup-shorthand.js",
    ] {
        let path = format!("test/language/expressions/object/{name}");
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

#[test]
fn object_instructions_require_scope_and_operands() {
    use temiya::bytecode::{
        Bytecode, Constant, Instruction, ScopeDescriptor, ScopeKind, VerifyErrorKind,
    };
    let code = Bytecode {
        constants: vec![],
        instructions: vec![Instruction::CreateObject, Instruction::Return],
        local_count: 0,
    };
    assert_eq!(
        code.verify().unwrap_err().kind(),
        VerifyErrorKind::ScopeMismatch
    );
    for op in [
        Instruction::CoercePropertyKey,
        Instruction::DefineObjectProperty {
            prototype_setter: false,
        },
        Instruction::SpreadObject,
    ] {
        let code = Bytecode {
            constants: vec![Constant::Scope(ScopeDescriptor {
                kind: ScopeKind::Global,
                strict: false,
                bindings: vec![],
                functions: vec![],
            })],
            instructions: vec![Instruction::EnterScope(0), op, Instruction::Return],
            local_count: 0,
        };
        assert_eq!(
            code.verify().unwrap_err().kind(),
            VerifyErrorKind::StackUnderflow
        );
    }
}
