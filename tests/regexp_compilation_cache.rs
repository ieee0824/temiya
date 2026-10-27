use std::{cell::RefCell, rc::Rc};
use temiya::{
    Engine,
    embedding::ScriptBackend,
    runtime::{JsString, Object, RegExp, RuntimeError, RuntimeErrorKind, Value},
};

#[test]
fn cached_construction_keeps_coercion_order_identity_and_abrupt_values() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let calls = Rc::new(RefCell::new(String::new()));
        for (name, marker, text) in [("pattern", 'P', "a"), ("flags", 'F', "g")] {
            let object = Object::new();
            let observed = calls.clone();
            object.set_to_string_hook(move || {
                observed.borrow_mut().push(marker);
                Ok(Value::String(text.into()))
            });
            engine
                .set_global(
                    &realm,
                    name,
                    engine.value(&realm, Value::Object(object)).unwrap(),
                )
                .unwrap();
        }
        let script = engine
            .compile(&realm, "new RegExp(pattern, flags);")
            .unwrap();
        let first = engine.evaluate(&script).unwrap();
        let second = engine.evaluate(&script).unwrap();
        assert_eq!(&*calls.borrow(), "PFPF");
        let regexp = |value: &Value| match value {
            Value::Object(object) => RegExp::from_object(object.clone()).unwrap(),
            _ => panic!("expected RegExp object"),
        };
        let first = regexp(first.value());
        let second = regexp(second.value());
        assert_ne!(first.object(), second.object());
        let input = JsString::from("aa");
        assert_eq!(first.exec(&input).unwrap().unwrap().index, 0);
        assert_eq!(second.exec(&input).unwrap().unwrap().index, 0);
        assert_eq!(first.exec(&input).unwrap().unwrap().index, 1);
        let reason = Value::Object(Object::new());
        let thrown = reason.clone();
        let observed = calls.clone();
        let flags = Object::new();
        flags.set_to_string_hook(move || {
            observed.borrow_mut().push('E');
            Err(RuntimeError::thrown(thrown.clone()))
        });
        engine
            .set_global(
                &realm,
                "flags",
                engine.value(&realm, Value::Object(flags)).unwrap(),
            )
            .unwrap();
        let error = engine.evaluate(&script).unwrap_err();
        assert!(error.thrown().unwrap().value().same_value(&reason));
        assert_eq!(&*calls.borrow(), "PFPFPE");
    }
}

#[test]
#[ignore = "requires pinned Test262 corpus; set TEST262_DIR"]
fn pinned_test262_regexp_compilation() {
    use std::process::Command;
    use temiya::test262::{TEST262_REVISION, TestCase, TestVariant};
    let corpus = std::env::var("TEST262_DIR").expect("set TEST262_DIR");
    let read = |path: &str| {
        let output = Command::new("git")
            .args(["-C", &corpus, "show", &format!("{TEST262_REVISION}:{path}")])
            .output()
            .expect("git show");
        assert!(output.status.success(), "missing pinned file {path}");
        String::from_utf8(output.stdout).unwrap()
    };
    let mut passed = 0;
    for name in [
        "15.10.4.1-2.js",
        "15.10.4.1-3.js",
        "S15.10.4.1_A5_T1.js",
        "S15.10.4.1_A5_T4.js",
        "S15.10.4.1_A5_T6.js",
        "S15.10.4.1_A5_T7.js",
        "prototype/exec/u-captured-value.js",
        "prototype/exec/S15.10.6.2_A2_T1.js",
    ] {
        let path = format!("test/built-ins/RegExp/{name}");
        let case = TestCase::parse(&path, read(&path)).unwrap();
        assert!(case.metadata.negative.is_none());
        assert!(
            !case
                .metadata
                .flags
                .iter()
                .any(|f| f == "async" || f == "raw")
        );
        for variant in case.variants() {
            assert_ne!(variant, TestVariant::Module);
            for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
                let engine = Engine::builder().script_backend(backend).build();
                let realm = engine.create_realm();
                for name in ["sta.js", "assert.js"]
                    .into_iter()
                    .chain(case.metadata.includes.iter().map(String::as_str))
                {
                    let setup = engine
                        .compile(&realm, &read(&format!("harness/{name}")))
                        .unwrap();
                    engine
                        .evaluate(&setup)
                        .unwrap_or_else(|e| panic!("harness {name}: {e:?}"));
                }
                let script = engine
                    .compile(&realm, &case.source_for(variant))
                    .unwrap_or_else(|e| panic!("{path} {variant:?} {backend:?}: {e}"));
                for pass in 0..2 {
                    engine.evaluate(&script).unwrap_or_else(|e| {
                        panic!("{path} {variant:?} {backend:?} pass {pass}: {e:?}")
                    });
                    passed += 1;
                    println!("PASS {path} {variant:?} {backend:?} pass {pass}");
                }
            }
        }
    }
    println!("Test262 {TEST262_REVISION}: {passed} passed; 0 failed; 0 skipped in selection");
}

#[test]
fn reused_capture_storage_clears_unmatched_groups_and_old_inputs() {
    let regexp = RegExp::new(JsString::from("(?<optional>a)?b"), "").unwrap();
    assert_eq!(
        regexp
            .exec(&JsString::from("ab"))
            .unwrap()
            .unwrap()
            .captures[1],
        Some(JsString::from("a"))
    );
    let unmatched = regexp.exec(&JsString::from("b")).unwrap().unwrap();
    assert_eq!(unmatched.captures, vec![Some(JsString::from("b")), None]);
    assert_eq!(
        unmatched.named_captures,
        vec![("optional".to_owned(), None)]
    );
    assert!(regexp.exec(&JsString::from("x")).unwrap().is_none());
    assert_eq!(
        regexp
            .exec(&JsString::from("ab"))
            .unwrap()
            .unwrap()
            .captures[1],
        Some(JsString::from("a"))
    );
}

#[test]
fn cached_matcher_preserves_utf16_boundaries_and_current_surrogate_errors() {
    let original = RegExp::new(JsString::from("(.)"), "gu").unwrap();
    let cached = RegExp::new(JsString::from("(.)"), "gu").unwrap();
    let lone = JsString::from_code_units(vec![0xd800]);
    cached.set_last_index(1);
    assert_eq!(
        cached.exec(&lone).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(cached.last_index(), 1);
    let input = JsString::from("😀a");
    assert!(cached.exec(&input).unwrap().is_none());
    assert_eq!(cached.last_index(), 0);
    let matched = cached.exec(&input).unwrap().unwrap();
    assert_eq!(matched.index, 0);
    assert_eq!(
        matched.captures,
        vec![Some(JsString::from("😀")), Some(JsString::from("😀"))]
    );
    assert_eq!(cached.last_index(), 2);
    assert_eq!(original.last_index(), 0);
    assert_eq!(
        RegExp::new(lone.clone(), "u").unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(
        RegExp::new(lone, "gg").unwrap_err().kind(),
        RuntimeErrorKind::SyntaxError
    );
}

#[test]
fn non_ascii_cache_keys_preserve_scalar_patterns_and_independent_state() {
    for _ in 0..2 {
        let astral = RegExp::new(JsString::from("😀"), "gu").unwrap();
        let latin = RegExp::new(JsString::from("é"), "gu").unwrap();
        let input = JsString::from("😀é");
        assert_eq!(astral.source(), JsString::from("😀"));
        assert_eq!(latin.source(), JsString::from("é"));
        assert_eq!(astral.last_index(), 0);
        assert_eq!(latin.last_index(), 0);
        assert_eq!(astral.exec(&input).unwrap().unwrap().index, 0);
        assert_eq!(astral.last_index(), 2);
        assert_eq!(latin.exec(&input).unwrap().unwrap().index, 2);
        assert_eq!(latin.last_index(), 3);
        assert!(astral.exec(&input).unwrap().is_none());
        assert_eq!(astral.last_index(), 0);
        assert_eq!(latin.last_index(), 3);
    }
}

#[test]
fn builtin_exec_owns_each_capture_array_and_clears_optional_groups() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine
            .compile(
                &realm,
                "var r=new RegExp('(a)?b','g');var first=r.exec('ab b');\
             var second=r.exec('ab b');var failure=r.exec('ab b');\
             first!==second&&first[0]==='ab'&&first[1]==='a'&&\
             second[0]==='b'&&second[1]===undefined&&failure===null&&\
             r.exec('ab')[1]==='a'&&first[1]==='a';",
            )
            .unwrap();
        for _ in 0..2 {
            assert!(
                engine
                    .evaluate(&script)
                    .unwrap()
                    .value()
                    .same_value(&Value::Boolean(true))
            );
        }
    }
}

#[test]
fn short_ascii_input_and_unicode_fallback_keep_owned_captures_at_buffer_boundary() {
    for len in [63, 64, 65] {
        let input = JsString::from("a".repeat(len));
        let regexp = RegExp::new(JsString::from("(a+)"), "gu").unwrap();
        let found = regexp.exec(&input).unwrap().unwrap();
        assert_eq!(found.index, 0);
        assert_eq!(
            found.captures,
            vec![Some(input.clone()), Some(input.clone())]
        );
        assert_eq!(regexp.last_index(), len);
        assert!(regexp.exec(&input).unwrap().is_none());
        assert_eq!(regexp.last_index(), 0);
        assert_eq!(found.captures[0], Some(input));
    }
    let input = JsString::from(format!("{}😀{}", "a".repeat(31), "a".repeat(31)));
    assert_eq!(input.code_units().len(), 64);
    let regexp = RegExp::new(JsString::from("(?<value>.)"), "gu").unwrap();
    regexp.set_last_index(31);
    let found = regexp.exec(&input).unwrap().unwrap();
    assert_eq!(found.index, 31);
    assert_eq!(found.captures[0], Some(JsString::from("😀")));
    assert_eq!(found.named_captures[0].1, Some(JsString::from("😀")));
    assert_eq!(regexp.last_index(), 33);
}
