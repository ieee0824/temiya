use temiya::{
    Engine,
    embedding::ExceptionKind,
    runtime::{Object, RuntimeError, RuntimeErrorKind, Value},
};

fn check(source: &str) {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine.compile(&realm, source).unwrap();
    let result = engine.evaluate(&script).unwrap();
    assert!(matches!(result.value(), Value::Boolean(true)), "{source}");
}

#[test]
fn function_calls_preserve_every_throw_value_through_catch_and_finally() {
    for value in [
        "undefined",
        "null",
        "false",
        "42",
        "-0",
        "NaN",
        "'text'",
        "7n",
        "{}",
    ] {
        check(&format!(
            "var marker = {value}; var log = ''; var caught; \
             function fail() {{ throw marker; }} \
             function relay() {{ try {{ fail(); }} finally {{ log += 'f'; }} }} \
             try {{ relay(); }} catch (e) {{ caught = e; log += 'c'; }} \
             ((caught === marker && (marker !== 0 || 1/caught === 1/marker)) || \
              (caught !== caught && marker !== marker)) && log === 'fc';"
        ));
    }
}

#[test]
fn accessor_and_constructor_throws_keep_identity_without_coercion() {
    check(
        "var marker = { toString() { throw 'coercion'; } }; var count = 0; \
         var o = { get x() { throw marker; }, set x(v) { throw marker; } }; \
         function Fail() { throw marker; } \
         try { o.x; } catch (e) { if (e === marker) count++; } \
         try { o.x = 1; } catch (e) { if (e === marker) count++; } \
         try { new Fail(); } catch (e) { if (e === marker) count++; } count === 3;",
    );
}

#[test]
fn finally_runs_after_errors_from_try_and_catch_and_can_replace_them() {
    check(
        "var log = ''; var marker = {}; var caught; \
         function fail() { throw marker; } \
         try { try { fail(); } catch (e) { log += 'c'; null.x; } \
               finally { log += 'f'; } } \
         catch (e) { caught = e; } \
         log === 'cf' && caught.constructor === TypeError;",
    );
    check(
        "function overridden() { try { null.x; } finally { return 42; } } \
         function overriddenCatch() { try { throw 1; } catch (e) { null.x; } \
             finally { return 43; } } \
         overridden() === 42 && overriddenCatch() === 43;",
    );
    check(
        "var marker = {}; var caught; \
         function replaced() { try { null.x; } finally { throw marker; } } \
         try { replaced(); } catch (e) { caught = e; } caught === marker;",
    );
    check(
        "var n = 0; while (n < 2) { try { null.x; } finally { n++; continue; } } \
         while (true) { try { null.x; } finally { break; } } n === 2;",
    );
}

#[test]
fn native_errors_are_catchable_with_intrinsic_prototypes() {
    for (expression, constructor) in [
        ("null.x", "TypeError"),
        ("missingBinding", "ReferenceError"),
        ("Array(-1)", "RangeError"),
        ("RegExp('[')", "SyntaxError"),
    ] {
        check(&format!(
            "var result = false; try {{ {expression}; }} catch (e) {{ \
               result = e.constructor === {constructor} && \
                        typeof e.message === 'string' && e.name === '{constructor}'; \
             }} result;"
        ));
    }
    check(
        "var original = TypeError; TypeError = function() { throw 99; }; \
         var result = false; try { null.x; } catch (e) { result = e.constructor === original; } \
         result;",
    );
}

#[test]
fn embedding_and_module_boundaries_retain_owned_values() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let marker = engine
        .value(
            &realm,
            Value::Object(Object::new_in(engine.allocation_context())),
        )
        .unwrap();
    engine.set_global(&realm, "marker", marker.clone()).unwrap();
    let script = engine
        .compile(&realm, "function fail() { throw marker; } fail();")
        .unwrap();
    let exception = engine.evaluate(&script).unwrap_err();
    assert_eq!(exception.kind(), ExceptionKind::ThrownValue);
    let thrown = exception.thrown().unwrap();
    assert!(thrown.shares_realm_with(&marker));
    assert!(thrown.value().same_value(marker.value()));
    let module = engine
        .load_module(&realm, "entry", |_| Ok("throw marker;".into()))
        .unwrap();
    let exception = engine.evaluate_module(&module).unwrap_err();
    assert_eq!(exception.kind(), ExceptionKind::ThrownValue);
    assert!(exception.thrown().unwrap().shares_realm_with(&marker));
    assert!(
        exception
            .thrown()
            .unwrap()
            .value()
            .same_value(marker.value())
    );
}

#[test]
fn called_function_materializes_native_errors_in_its_own_realm() {
    let engine = Engine::default();
    let origin = engine.create_realm();
    let caller = engine.create_realm();
    let make = engine
        .compile(&origin, "(function fail() { null.x; });")
        .unwrap();
    let Value::Object(function) = engine.evaluate(&make).unwrap().into_value() else {
        panic!()
    };
    // Rewrap explicitly at the host boundary; public handles remain Realm-tagged.
    engine
        .set_global(
            &caller,
            "fail",
            engine.value(&caller, Value::Object(function)).unwrap(),
        )
        .unwrap();
    let test = engine
        .compile(
            &caller,
            "var caught; try { fail(); } catch (e) { caught = e; } caught;",
        )
        .unwrap();
    let result = engine.evaluate(&test).unwrap();
    let expected = engine.compile(&origin, "TypeError.prototype;").unwrap();
    let expected = engine.evaluate(&expected).unwrap();
    let Value::Object(error) = result.value() else {
        panic!()
    };
    assert!(Value::Object(error.prototype().unwrap()).same_value(expected.value()));
}

#[test]
fn generator_throw_values_survive_both_active_and_completed_boundaries() {
    check(
        "function* gen() { throw null; } var caught = false; \
         try { gen().next(); } catch (e) { caught = e === null; } caught;",
    );
    check(
        "function* gen() {} var g = gen(); g.next(); var caught = false; \
         try { g.throw(42); } catch (e) { caught = e === 42; } caught;",
    );
}

#[test]
fn internal_failures_cannot_be_swallowed_by_catch_or_finally() {
    // Derived classes remain unsupported; base classes now execute normally.
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine.compile(&realm,
        "function f() { try { (class extends Object {}); } catch (e) { return true; } finally { return true; } } f();"
    ).unwrap();
    assert_eq!(
        engine.evaluate(&script).unwrap_err().kind(),
        ExceptionKind::Engine
    );
    let thrown = RuntimeError::thrown(Value::Null);
    assert_eq!(thrown.kind(), RuntimeErrorKind::ThrownValue);
    assert!(matches!(thrown.thrown_value(), Some(Value::Null)));
}

/// Uses upstream harness and tests unchanged, read from immutable Git objects.
/// This is a focused regression suite, not a whole-Test262 conformance runner.
#[test]
#[ignore = "requires the pinned Test262 corpus; see docs/runtime.md"]
fn pinned_test262_exception_regressions() {
    use std::process::Command;
    use temiya::test262::{TEST262_REVISION, TestCase, TestVariant};

    let corpus = std::env::var("TEST262_DIR").expect("set TEST262_DIR");
    let read = |path: &str| {
        let output = Command::new("git")
            .args(["-C", &corpus, "show", &format!("{TEST262_REVISION}:{path}")])
            .output()
            .expect("git show");
        assert!(output.status.success(), "missing pinned source {path}");
        String::from_utf8(output.stdout).unwrap()
    };
    let harness = format!("{}\n{}", read("harness/sta.js"), read("harness/assert.js"));
    let cases = [
        "completion-values-fn-finally-normal.js",
        "completion-values-fn-finally-abrupt.js",
        "S12.14_A7_T1.js",
        "S12.14_A8.js",
        "S12.14_A9_T1.js",
        "S12.14_A11_T3.js",
        "optional-catch-binding-throws.js",
    ];
    let mut passed = 0;
    for name in cases {
        let path = format!("test/language/statements/try/{name}");
        let case = TestCase::parse(&path, read(&path)).unwrap();
        assert!(case.metadata.negative.is_none());
        assert!(case.metadata.includes.is_empty());
        assert!(
            !case
                .metadata
                .flags
                .iter()
                .any(|flag| flag == "async" || flag == "raw")
        );
        for variant in case.variants() {
            assert_ne!(variant, TestVariant::Module);
            let engine = Engine::default();
            let realm = engine.create_realm();
            let setup = engine.compile(&realm, &harness).unwrap();
            engine.evaluate(&setup).unwrap();
            let script = engine
                .compile(&realm, &case.source_for(variant))
                .unwrap_or_else(|error| panic!("{path} {variant:?} compile: {error}"));
            engine
                .evaluate(&script)
                .unwrap_or_else(|error| panic!("{path} {variant:?} evaluate: {error:?}"));
            passed += 1;
            println!("PASS {path} {variant:?}");
        }
    }
    println!("Test262 {TEST262_REVISION}: {passed} passed; 0 failed; 0 skipped");
}
