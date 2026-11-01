use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, RuntimeErrorKind, StatementEvaluator, Value},
};
fn execute(
    realm: &Realm,
    source: &str,
    vm: bool,
) -> Result<Completion, temiya::runtime::RuntimeError> {
    if vm {
        VirtualMachine::execute_in_realm(&BytecodeCompiler::compile(source).unwrap(), realm)
    } else {
        StatementEvaluator::new(realm.clone()).evaluate_program(&parse_script(source).unwrap())
    }
}
fn result(realm: &Realm, source: &str, vm: bool) -> Value {
    match execute(realm, source, vm).unwrap() {
        Completion::Normal(Some(value)) => value,
        value => panic!("{value:?}"),
    }
}
#[test]
fn numeric_comparison_equality_shift_and_power_boundaries_match() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for (source, expected) in [
        ("45 % 43;", Value::Number(2.0)),
        ("2 ** 5;", Value::Number(32.0)),
        ("5 & 3;", Value::Number(1.0)),
        ("5 | 2;", Value::Number(7.0)),
        ("5 ^ 3;", Value::Number(6.0)),
        ("1 << 33;", Value::Number(2.0)),
        ("-4 >> 1;", Value::Number(-2.0)),
        ("-1 >>> 31;", Value::Number(1.0)),
        ("1 ** (1/0);", Value::Number(f64::NAN)),
        ("(0/0) ** 0;", Value::Number(1.0)),
        (
            "9007199254740993n > 9007199254740992;",
            Value::Boolean(true),
        ),
        (
            "9007199254740993n == 9007199254740992;",
            Value::Boolean(false),
        ),
        (
            "9007199254740992n == 9007199254740992;",
            Value::Boolean(true),
        ),
        ("0n < 5e-324;", Value::Boolean(true)),
        ("0n > -5e-324;", Value::Boolean(true)),
        ("-2n < -1.5;", Value::Boolean(true)),
        ("2.5 > 2n;", Value::Boolean(true)),
        ("1n < 'no';", Value::Boolean(false)),
        ("1n >= 'no';", Value::Boolean(false)),
        ("'02' < 3n;", Value::Boolean(true)),
        ("'2' > '11';", Value::Boolean(true)),
        ("0n == -0;", Value::Boolean(true)),
        ("42n == '42';", Value::Boolean(true)),
        ("1n == (1/0);", Value::Boolean(false)),
        ("1n >= (0/0);", Value::Boolean(false)),
        ("1n >> 999999999999999999999n;", Value::BigInt(0.into())),
        ("-1n >> 999999999999999999999n;", Value::BigInt((-1).into())),
        ("1n << -999999999999999999999n;", Value::BigInt(0.into())),
        ("1n ** 999999999999999999999n;", Value::BigInt(1.into())),
        ("null == undefined;", Value::Boolean(true)),
        ("42 !== '42';", Value::Boolean(true)),
    ] {
        for vm in [false, true] {
            assert!(
                result(&Realm::ordinary_in(&allocation_context), source, vm).same_value(&expected),
                "{source}"
            );
        }
    }
    for (source, kind) in [
        ("1n + 1;", RuntimeErrorKind::TypeError),
        ("1n >>> 0n;", RuntimeErrorKind::TypeError),
        ("1n / 0n;", RuntimeErrorKind::RangeError),
        ("2n ** -1n;", RuntimeErrorKind::RangeError),
    ] {
        for vm in [false, true] {
            assert_eq!(
                execute(&Realm::ordinary_in(&allocation_context), source, vm)
                    .unwrap_err()
                    .kind(),
                kind,
                "{source}"
            );
        }
    }
}
#[test]
fn logical_binary_and_compound_assignments_preserve_values_and_skip_rhs() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for (source, expected) in [
        ("let n=0;false && ++n;n;", Value::Number(0.0)),
        ("let n=0;'yes' || ++n;n;", Value::Number(0.0)),
        ("let n=0;0 ?? ++n;n;", Value::Number(0.0)),
        ("null ?? 42;", Value::Number(42.0)),
        ("false || 42;", Value::Number(42.0)),
        ("let n=20;n+=22;", Value::Number(42.0)),
        ("let n=6;n*=7;", Value::Number(42.0)),
        ("let n=84;n/=2;", Value::Number(42.0)),
        ("let n=44;n-=2;", Value::Number(42.0)),
        ("let n=2;n**=5;", Value::Number(32.0)),
        ("let n=44;n%=43;", Value::Number(1.0)),
        ("let n=1;n<<=5;", Value::Number(32.0)),
        ("let n=-4;n>>=1;", Value::Number(-2.0)),
        ("let n=-1;n>>>=31;", Value::Number(1.0)),
        ("let n=5;n&=3;", Value::Number(1.0)),
        ("let n=5;n|=2;", Value::Number(7.0)),
        ("let n=5;n^=3;", Value::Number(6.0)),
        ("const n=0;n&&=missing;", Value::Number(0.0)),
        ("const n=42;n||=missing;", Value::Number(42.0)),
        ("const n='';n??=missing;", Value::String("".into())),
        ("let n;n??=42;", Value::Number(42.0)),
        ("let n=0;n||=42;", Value::Number(42.0)),
        ("let n=1;n&&=42;", Value::Number(42.0)),
        ("let n=1n;n+=41n;", Value::BigInt(42.into())),
    ] {
        for vm in [false, true] {
            assert!(
                result(&Realm::ordinary_in(&allocation_context), source, vm).same_value(&expected),
                "{source}"
            );
        }
    }
}
#[test]
fn compound_references_get_once_convert_key_once_and_keep_original_base() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let setup = "var log='';var count=0;var stored;var obj=new Object();var key=new Object();key[Symbol.toPrimitive]=function(h){count++;log+=h;return count===1?'x':'wrong';};var desc=new Object();desc.get=function(){if(this!==original){throw 99;}log+='G';return 20;};desc.set=function(v){if(this!==original){throw 99;}log+='S';stored=v;};desc.configurable=true;Object.defineProperty(obj,'x',desc);var original=obj;var rhs=function(){log+='R';obj=new Object();return 22;};";
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        let source = format!("{setup}var answer=obj[key]+=rhs();answer+':'+stored+':'+log;");
        assert!(result(&realm, &source, vm).same_value(&Value::String("42:42:stringGRS".into())));
        assert!(result(&realm, "count;", vm).same_value(&Value::Number(1.0)));
    }
}
#[test]
fn numeric_and_relational_coercion_have_distinct_abrupt_ordering() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for (operator, expected_log) in [
        ("*", "L"),
        ("-", "L"),
        ("<", "LR"),
        (">=", "LR"),
        ("+", "LR"),
    ] {
        for vm in [false, true] {
            let realm = Realm::ordinary_in(&allocation_context);
            let source = format!(
                "var log='';var a=new Object();var b=new Object();a[Symbol.toPrimitive]=function(h){{log+='L';return Symbol.iterator;}};b[Symbol.toPrimitive]=function(h){{log+='R';return 2;}};a {operator} b;"
            );
            assert_eq!(
                execute(&realm, &source, vm).unwrap_err().kind(),
                RuntimeErrorKind::TypeError
            );
            assert!(result(&realm, "log;", vm).same_value(&Value::String(expected_log.into())));
        }
    }
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        let source = "var count=0;var a=new Object();a[Symbol.toPrimitive]=function(h){count++;return 42;};var x=a==null;var y=a==undefined;count;";
        assert!(result(&realm, source, vm).same_value(&Value::Number(0.0)));
        assert!(result(&realm, "a==42;", vm).same_value(&Value::Boolean(true)));
        assert!(result(&realm, "count;", vm).same_value(&Value::Number(1.0)));
    }
}
#[test]
fn in_checks_rhs_before_key_coercion_and_uses_string_hint() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        let source = "var log='';var key=new Object();key[Symbol.toPrimitive]=function(h){log+=h;return 'x';};key in 42;";
        assert_eq!(
            execute(&realm, source, vm).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
        assert!(result(&realm, "log;", vm).same_value(&Value::String("".into())));
        assert!(
            result(&realm, "var obj=new Object();obj.x=42;key in obj;", vm)
                .same_value(&Value::Boolean(true))
        );
        assert!(result(&realm, "log;", vm).same_value(&Value::String("string".into())));
    }
}
#[test]
fn instanceof_honors_custom_methods_bound_targets_and_primitive_order() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for (source, expected) in [
        (
            "function C(x){this.x=x;}var B=C.bind(null,42);var o=new B();o instanceof B && o instanceof C && o.x===42;",
            Value::Boolean(true),
        ),
        (
            "var matcher=new Object();matcher[Symbol.hasInstance]=function(x){return this===matcher && x===42;};42 instanceof matcher;",
            Value::Boolean(true),
        ),
        (
            "var count=0;var C=()=>42;var d=new Object();d.get=function(){count++;throw 17;};Object.defineProperty(C,'prototype',d);var answer=42 instanceof C;count;",
            Value::Number(0.0),
        ),
        (
            "Function.prototype[Symbol.hasInstance].call(new Object(),42);",
            Value::Boolean(false),
        ),
    ] {
        for vm in [false, true] {
            assert!(
                result(&Realm::ordinary_in(&allocation_context), source, vm).same_value(&expected),
                "{source}"
            );
        }
    }
    let source = "function C(){}var d=new Object();d.value=function(x){return x===42;};Object.defineProperty(C,Symbol.hasInstance,d);var B=C.bind(null);42 instanceof B;";
    for vm in [false, true] {
        assert!(
            result(&Realm::ordinary_in(&allocation_context), source, vm)
                .same_value(&Value::Boolean(true))
        );
    }
}
#[test]
fn proxy_prototype_errors_propagate_through_instanceof() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        let source = "var handler=new Object();handler.getPrototypeOf=function(){throw 17;};var proxy=new Proxy(new Object(),handler);proxy instanceof Object;";
        let error = execute(&realm, source, vm).unwrap_err();
        assert!(
            error
                .thrown_value()
                .unwrap()
                .same_value(&Value::Number(17.0))
        );
    }
}

#[test]
fn logical_property_assignments_skip_rhs_and_setter_after_one_get() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for (operator, old, expected_log) in [
        ("&&=", "0", "KG"),
        ("||=", "42", "KG"),
        ("??=", "false", "KG"),
        ("&&=", "1", "KGRS"),
        ("||=", "0", "KGRS"),
        ("??=", "null", "KGRS"),
    ] {
        for vm in [false, true] {
            let source = format!(
                "var log='';var obj=new Object();var key=new Object();key[Symbol.toPrimitive]=function(){{log+='K';return 'x';}};var d=new Object();d.get=function(){{log+='G';return {old};}};d.set=function(v){{log+='S';}};Object.defineProperty(obj,'x',d);var rhs=function(){{log+='R';return 42;}};obj[key] {operator} rhs();log;"
            );
            assert!(
                result(&Realm::ordinary_in(&allocation_context), &source, vm)
                    .same_value(&Value::String(expected_log.into())),
                "{operator} {old}"
            );
        }
    }
}

#[test]
fn bound_constructor_releases_native_target_after_last_root() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::rc::Rc;
    use temiya::runtime::{IntrinsicId, Object};
    let realm = Realm::ordinary_in(&allocation_context);
    let marker = Rc::new(());
    let weak = Rc::downgrade(&marker);
    let target = Object::native_callable_constructor(
        &allocation_context,
        move |_, _| {
            let _keep = &marker;
            Ok(Value::Undefined)
        },
        |_, target| Ok(Object::new_in(target.allocation_context())),
    );
    target.set_prototype(Some(realm.intrinsic(IntrinsicId::FunctionPrototype)));
    let Value::Object(bind) = realm
        .intrinsic(IntrinsicId::FunctionPrototype)
        .get(&"bind".into())
        .unwrap()
    else {
        panic!("missing bind")
    };
    let bound = bind.call(Value::Object(target), &[Value::Null]).unwrap();
    assert!(weak.upgrade().is_some());
    drop(bound);
    allocation_context.collect_garbage().unwrap();
    assert!(weak.upgrade().is_none());
}

#[test]
fn fresh_function_and_instance_prototype_links_do_not_walk_proxy_chains() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for source in [
        "var count=0;var handler=new Object();handler.getPrototypeOf=function(){count++;return null;};var proto=new Proxy(new Object(),handler);function C(){}Object.setPrototypeOf(C,proto);count=0;var args=new Array();Reflect.apply(Function.prototype.bind,C,args);count;",
        "var count=0;var handler=new Object();handler.getPrototypeOf=function(){count++;return null;};var proto=new Proxy(new Object(),handler);function C(){}C.prototype=proto;count=0;new C();count;",
    ] {
        for vm in [false, true] {
            assert!(
                result(&Realm::ordinary_in(&allocation_context), source, vm)
                    .same_value(&Value::Number(0.0)),
                "{source}"
            );
        }
    }
}

#[test]
#[ignore = "requires pinned Test262 corpus; records unfinished binary/compound operator integration gates"]
fn pinned_test262_binary_audit() {
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
        "expressions/equals/bigint-and-number-extremes.js",
        "expressions/equals/bigint-and-string.js",
        "expressions/equals/bigint-and-object.js",
        "expressions/equals/coerce-symbol-to-prim-return-prim.js",
        "expressions/greater-than/bigint-and-number-extremes.js",
        "expressions/greater-than/bigint-and-incomparable-string.js",
        "expressions/less-than/bigint-and-number.js",
        "expressions/exponentiation/bigint-arithmetic.js",
        "expressions/exponentiation/bigint-negative-exponent-throws.js",
        "expressions/exponentiation/bigint-wrapped-values.js",
        "expressions/unsigned-right-shift/bigint-and-number.js",
        "expressions/instanceof/symbol-hasinstance-invocation.js",
        "expressions/instanceof/symbol-hasinstance-not-callable.js",
        "expressions/logical-assignment/lgcl-or-assignment-operator-unresolved-rhs.js",
        "expressions/logical-assignment/lgcl-and-assignment-operator-unresolved-lhs.js",
        "expressions/logical-assignment/lgcl-nullish-assignment-operator-unresolved-rhs-put.js",
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
        "binary operator upstream integration remains incomplete"
    );
}

#[test]
fn strict_equality_preserves_identity_types_and_never_invokes_proxy_or_coercion_hooks() {
    let source = "function nope(){throw 42;}let a={valueOf:nope,toString:nope},b={};let proxy=new Proxy(a,{get:nope});let token=Symbol(),other=Symbol();proxy===proxy&&proxy!==a&&a!==b&&a!==42&&null!==undefined&&'42'!==42&&42n!==42&&42n===42n&&token===token&&token!==other&&'\\ud800'==='\\ud800'&&'\\ud800'!=='\\ud801'&&0===-0&&(0/0)!==(0/0)&&!(a===proxy);";
    let code = BytecodeCompiler::compile(source).unwrap();
    assert!(
        code.bytecode()
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, temiya::bytecode::Instruction::StrictEqual))
    );
    for vm in [false, true] {
        assert!(
            matches!(result(&Realm::ordinary(), source, vm), Value::Boolean(true)),
            "VM: {vm}"
        );
    }
}

#[test]
fn default_instance_shortcut_preserves_prototype_effects_and_dynamic_fallbacks() {
    let source = "function run(){function C(){}let x=new C(),B=C.bind(null);B.prototype=42;let good=x instanceof C&&x instanceof B;let effects=0,marker={};let A=()=>0;Object.defineProperty(A,'prototype',{get:function(){effects++;throw marker;}});good=good&&!(42 instanceof A)&&effects===0;try{x instanceof A;}catch(e){good=good&&e===marker&&effects===1;}let custom={};Object.defineProperty(custom,Symbol.hasInstance,{get:function(){effects++;return function(v){return this===C&&v===x;};}});Object.setPrototypeOf(C,custom);good=good&&(x instanceof C)&&effects===2;Object.setPrototypeOf(C,Function.prototype);let trace='';let P=new Proxy(C,{get:function(target,key,receiver){trace+=key===Symbol.hasInstance?'h':'p';return Reflect.get(target,key,receiver);}});good=good&&(x instanceof P)&&trace==='hp';let rev=Proxy.revocable(x,{});rev.revoke();let caught=false;try{rev.proxy instanceof C;}catch(e){caught=e instanceof TypeError;}Object.defineProperty(C,Symbol.hasInstance,{value:undefined});return good&&caught&&(x instanceof C);}run();";
    for vm in [false, true] {
        assert!(
            matches!(result(&Realm::ordinary(), source, vm), Value::Boolean(true)),
            "VM: {vm}"
        );
    }
}
