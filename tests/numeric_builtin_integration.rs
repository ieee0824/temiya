use temiya::{
    Engine,
    runtime::{IntrinsicId, Object, Realm, RuntimeErrorKind, Value},
};

fn evaluate(source: &str) -> Value {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine.compile(&realm, source).unwrap();
    engine.evaluate(&script).unwrap().value().clone()
}

#[test]
fn engine_exposes_bigint_wrappers_radices_and_safe_integer_boundaries() {
    for source in [
        "Object(42n).valueOf() === 42n;",
        "(new Object(-42n)).valueOf() === -42n;",
        "BigInt.prototype.toString.call(Object(-255n), 16) === '-ff';",
        "(35n).toString(36) === 'z';",
        "(16n).toString(2.9) === '10000';",
        "(42n).toString(undefined) === '42';",
        "Number.isSafeInteger(9007199254740991) && Number.isSafeInteger(-9007199254740991) && !Number.isSafeInteger(9007199254740992) && !Number.isSafeInteger(-9007199254740992);",
        "Number.isSafeInteger(-0) && !Number.isSafeInteger(NaN) && !Number.isSafeInteger(Infinity) && !Number.isSafeInteger(1.5);",
        "!Number.isSafeInteger('42') && !Number.isSafeInteger(42n) && !Number.isSafeInteger(Object(42n)) && !Number.isSafeInteger(new Number(42));",
        "let calls=0; let input={valueOf(){calls++;return 2;}}; !Number.isSafeInteger(input) && calls===0;",
        "let calls=0; let radix={[Symbol.toPrimitive](hint){calls++;return hint==='number'?16:2;}}; (255n).toString(radix)==='ff' && calls===1;",
    ] {
        assert!(matches!(evaluate(source), Value::Boolean(true)), "{source}");
    }
}

#[test]
fn bigint_radix_brand_checks_and_abrupt_order_are_observable() {
    let context = temiya::runtime::AllocationContext::new();
    for source in [
        "let log=''; let radix={valueOf(){log+='R';return 16;}}; let caught=false; try {BigInt.prototype.toString.call({},radix);} catch(e){caught=true;} caught && log==='';",
        "let marker={}; let caught=false; try {(1n).toString({valueOf(){throw marker;}});} catch(e){caught=e===marker;} caught;",
        "let log=''; let radix={valueOf(){log+='V';return {};},toString(){log+='S';return '16';}}; (255n).toString(radix)==='ff' && log==='VS';",
    ] {
        assert!(matches!(evaluate(source), Value::Boolean(true)), "{source}");
    }
    let realm = Realm::ordinary_in(&context);
    let prototype = realm.intrinsic(IntrinsicId::BigIntPrototype);
    let Value::Object(to_string) = prototype.get(&"toString".into()).unwrap() else {
        panic!()
    };
    let Value::Object(value_of) = prototype.get(&"valueOf".into()).unwrap() else {
        panic!()
    };
    let inherited = Object::new_in(&context);
    inherited.set_prototype(Some(prototype.clone()));
    let wrapped = temiya::runtime::BigIntObject::new(&context, 42.into()).object();
    let proxy = temiya::runtime::ProxyObject::new(wrapped, Object::new_in(&context)).object();
    for receiver in [
        Value::Undefined,
        Value::Null,
        Value::Number(1.0),
        Value::Object(Object::new_in(&context)),
        Value::Object(inherited),
        Value::Object(proxy),
        Value::Object(prototype),
    ] {
        for function in [&value_of, &to_string] {
            assert_eq!(
                function.call(receiver.clone(), &[]).unwrap_err().kind(),
                RuntimeErrorKind::TypeError
            );
        }
    }
    for radix in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -1.0,
        0.0,
        1.99,
        37.0,
    ] {
        assert_eq!(
            to_string
                .call(Value::BigInt(1.into()), &[Value::Number(radix)])
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::RangeError
        );
    }
    for radix in [
        Value::BigInt(2.into()),
        Value::Symbol(temiya::runtime::Symbol::new(None)),
    ] {
        assert_eq!(
            to_string
                .call(Value::BigInt(1.into()), &[radix])
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::TypeError
        );
    }
}

#[test]
fn numeric_constants_and_new_method_descriptors_match_the_public_contract() {
    let realm = Realm::ordinary();
    let number = realm.intrinsic(IntrinsicId::Number);
    for (name, expected) in [
        ("NaN", f64::NAN),
        ("POSITIVE_INFINITY", f64::INFINITY),
        ("NEGATIVE_INFINITY", f64::NEG_INFINITY),
        ("MAX_VALUE", f64::MAX),
        ("MIN_VALUE", f64::from_bits(1)),
        ("EPSILON", f64::EPSILON),
        ("MAX_SAFE_INTEGER", 9_007_199_254_740_991.0),
        ("MIN_SAFE_INTEGER", -9_007_199_254_740_991.0),
    ] {
        let d = number.get_own_property(&name.into()).unwrap();
        assert!(d.value.unwrap().same_value(&Value::Number(expected)));
        assert_eq!(
            (d.writable, d.enumerable, d.configurable),
            (Some(false), Some(false), Some(false))
        );
    }
    for (id, name, length) in [
        (IntrinsicId::Number, "isSafeInteger", 1.0),
        (IntrinsicId::BigIntPrototype, "valueOf", 0.0),
        (IntrinsicId::BigIntPrototype, "toString", 0.0),
    ] {
        let d = realm.intrinsic(id).get_own_property(&name.into()).unwrap();
        assert_eq!(
            (d.writable, d.enumerable, d.configurable),
            (Some(true), Some(false), Some(true))
        );
        let Value::Object(method) = d.value.unwrap() else {
            panic!()
        };
        assert_eq!(
            method.prototype(),
            Some(realm.intrinsic(IntrinsicId::FunctionPrototype))
        );
        assert!(!method.is_constructor());
        for (key, expected) in [
            ("name", Value::String(name.into())),
            ("length", Value::Number(length)),
        ] {
            let d = method.get_own_property(&key.into()).unwrap();
            assert!(d.value.unwrap().same_value(&expected));
            assert_eq!(
                (d.writable, d.enumerable, d.configurable),
                (Some(false), Some(false), Some(true))
            );
        }
    }
}

#[test]
fn bigint_valueof_accepts_an_actual_foreign_realm_wrapper() {
    let first = Realm::ordinary();
    let foreign = Realm::ordinary();
    let constructor = foreign.intrinsic(IntrinsicId::Object);
    let wrapped = constructor
        .call(Value::Undefined, &[Value::BigInt((-42).into())])
        .unwrap();
    let Value::Object(method) = first
        .intrinsic(IntrinsicId::BigIntPrototype)
        .get(&"valueOf".into())
        .unwrap()
    else {
        panic!()
    };
    assert!(
        method
            .call(wrapped, &[])
            .unwrap()
            .same_value(&Value::BigInt((-42).into()))
    );
}

#[test]
fn explicit_numeric_constructors_preserve_exact_integers_and_conversion_errors() {
    for source in [
        "BigInt(0)===0n && BigInt(-0)===0n && BigInt(9007199254740992)===9007199254740992n;",
        "BigInt(1e30)===1000000000000000019884624838656n;",
        "BigInt(-1e30)===-1000000000000000019884624838656n;",
        "Number(9007199254740993n)===9007199254740992 && (new Number(42n)).valueOf()===42;",
        "BigInt({valueOf(){return 42;}})===42n && Number({valueOf(){return 42n;}})===42;",
        "Object(42).valueOf()===42 && Object('hi').length===2;",
    ] {
        assert!(matches!(evaluate(source), Value::Boolean(true)), "{source}");
    }
    let realm = Realm::ordinary();
    let bigint = realm.intrinsic(IntrinsicId::BigInt);
    for number in [1.5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            bigint
                .call(Value::Undefined, &[Value::Number(number)])
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::RangeError
        );
    }
    // Object(Symbol) must retain its primitive slot; Proxy and ordinary objects
    // never acquire a numeric brand through their prototype alone.
    let symbol = temiya::runtime::Symbol::new(None);
    let Value::Object(boxed) = realm
        .intrinsic(IntrinsicId::Object)
        .call(Value::Undefined, &[Value::Symbol(symbol)])
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        boxed.prototype(),
        Some(realm.intrinsic(IntrinsicId::SymbolPrototype))
    );
}

#[test]
#[ignore = "requires pinned Test262 corpus; retains original numeric builtin outcomes including harness gaps"]
fn pinned_numeric_builtin_audit() {
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
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    assert_eq!(git(&["rev-parse", "HEAD"]).trim(), TEST262_REVISION);
    let paths = git(&[
        "ls-tree",
        "-r",
        "--name-only",
        TEST262_REVISION,
        "--",
        "test/built-ins/BigInt/prototype/toString",
        "test/built-ins/BigInt/prototype/valueOf",
        "test/built-ins/Number/isSafeInteger",
    ]);
    let paths = paths.lines().collect::<Vec<_>>();
    assert_eq!(paths.len(), 31);
    let read = |path: &str| git(&["show", &format!("{TEST262_REVISION}:{path}")]);
    let mut blobs = BTreeMap::new();
    let mut outcomes = vec![];
    for path in &paths {
        blobs.insert(
            path.to_string(),
            git(&["rev-parse", &format!("{TEST262_REVISION}:{path}")])
                .trim()
                .to_string(),
        );
        let case = TestCase::parse(*path, read(path)).unwrap();
        assert!(case.metadata.negative.is_none());
        assert!(
            !case
                .metadata
                .flags
                .iter()
                .any(|f| matches!(f.as_str(), "async" | "module" | "raw"))
        );
        for h in ["sta.js", "assert.js"]
            .into_iter()
            .chain(case.metadata.includes.iter().map(String::as_str))
        {
            let p = format!("harness/{h}");
            blobs.insert(
                p.clone(),
                git(&["rev-parse", &format!("{TEST262_REVISION}:{p}")])
                    .trim()
                    .to_string(),
            );
        }
        for variant in case.variants() {
            let run = || -> Result<(), String> {
                let realm = Realm::ordinary();
                // Standard Test262 host createRealm facility; original JS remains intact.
                let host = Object::new();
                host.create_data_property(
                    "createRealm".into(),
                    Value::Object(Object::native_function(|_, _| {
                        let other = Realm::ordinary();
                        let result = Object::new();
                        result.create_data_property(
                            "global".into(),
                            Value::Object(other.global_object()),
                        )?;
                        Ok(Value::Object(result))
                    })),
                )
                .unwrap();
                realm
                    .global_object()
                    .create_data_property("$262".into(), Value::Object(host))
                    .unwrap();
                for harness in ["sta.js", "assert.js"]
                    .into_iter()
                    .chain(case.metadata.includes.iter().map(String::as_str))
                {
                    ScriptRecord::parse(realm.clone(), &read(&format!("harness/{harness}")))
                        .map_err(|e| format!("harness parse {harness}: {e}"))?
                        .evaluate()
                        .map_err(|e| format!("harness evaluate {harness}: {e:?}"))?;
                }
                ScriptRecord::parse(realm, &case.source_for(variant))
                    .map_err(|e| format!("test parse: {e}"))?
                    .evaluate()
                    .map_err(|e| {
                        let message = e
                            .thrown_value()
                            .and_then(|v| {
                                if let Value::Object(o) = v {
                                    o.get(&"message".into()).ok()
                                } else {
                                    None
                                }
                            })
                            .map(|v| format!("{v:?}"))
                            .unwrap_or_default();
                        format!("evaluation: {e:?}; thrown message: {message}")
                    })?;
                Ok(())
            };
            let (status, message) = match run() {
                Ok(()) => ("pass", String::new()),
                Err(message) => ("fail", message),
            };
            outcomes.push(serde_json::json!({"path":path,"variant":format!("{variant:?}"),"status":status,"message":message}));
        }
    }
    let pass = outcomes.iter().filter(|r| r["status"] == "pass").count();
    let result = serde_json::json!({"revision":TEST262_REVISION,"selected_files":paths.len(),"fixture_git_blobs":blobs,"pass":pass,"fail":outcomes.len()-pass,"skipped":0,"outcomes":outcomes,"scope":"All original files in BigInt.prototype.toString/valueOf and Number.isSafeInteger directories; AST execution with original sta/assert/includes, fresh Realms and standard host createRealm facility. No fallback, assertion stripping or skips."});
    if let Ok(path) = std::env::var("TEST262_REPORT") {
        std::fs::write(path, serde_json::to_string_pretty(&result).unwrap()).unwrap();
    }
    println!("pass={pass}, fail={}", outcomes.len() - pass);
    assert_eq!(
        pass,
        outcomes.len(),
        "original numeric builtin gate remains incomplete"
    );
}
