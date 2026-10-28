use temiya::{Engine, runtime::Value};

fn check(source: &str) {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine.compile(&realm, source).unwrap();
    let result = engine.evaluate(&script).unwrap();
    assert!(
        matches!(result.value(), Value::Boolean(true)),
        "{source}\n{result:?}"
    );
}

#[test]
fn ordinary_methods_use_number_order_and_string_or_numeric_addition() {
    check(
        r#"
        var log = '';
        var a = {valueOf: function() {log += 'V'; return {};},
                 toString: function() {log += 'T'; return 'a';}};
        var b = {valueOf: function() {log += 'B'; return 2;}};
        var joined = a + b;
        var numeric = b + 3;
        joined === 'a2' && numeric === 5 && log === 'VTBB' &&
        ({valueOf: 0, toString: function() {return 'x';}}) + 1 === 'x1';
    "#,
    );
}

#[test]
fn exotic_method_observes_default_hint_receiver_and_one_argument() {
    check(
        r#"
        var log = '';
        var value = {valueOf: function() {throw 1;}, toString: function() {throw 2;}};
        value[Symbol.toPrimitive] = function(hint) {
            log += hint;
            if (this !== value || arguments.length !== 1) throw 3;
            return 'x';
        };
        value + 1 === 'x1' && 2 + value === '2x' && log === 'defaultdefault';
    "#,
    );
}

#[test]
fn operands_evaluate_before_left_then_right_conversion() {
    check(
        r#"
        var log = '';
        var a = {}; var b = {};
        a[Symbol.toPrimitive] = function() {log += 'L'; return 'a';};
        b[Symbol.toPrimitive] = function() {log += 'R'; return 'b';};
        function left() {log += 'A'; return a;}
        function right() {log += 'B'; return b;}
        var joined = left() + right();
        joined === 'ab' && log === 'ABLR';
    "#,
    );
}

#[test]
fn conversion_getters_and_abrupt_completions_preserve_order_and_throw_identity() {
    check(
        r#"
        var log = ''; var marker = {}; var caught = false;
        var a = {}; var b = {valueOf: function() {log += 'R'; return 'b';}};
        Object.defineProperty(a, Symbol.toPrimitive, {get: function() {log += 'G'; throw marker;}});
        function right() {log += 'E'; return b;}
        try {a + right();} catch (e) {caught = e === marker;}
        caught && log === 'EG';
    "#,
    );
    check(
        r#"
        var log = ''; var a = {}; var b = {};
        Object.defineProperty(a, 'valueOf', {get: function() {
            log += 'G'; return function() {log += 'V'; return {};};
        }});
        Object.defineProperty(a, 'toString', {get: function() {
            log += 'S'; return function() {log += 'T'; return 'a';};
        }});
        b[Symbol.toPrimitive] = function() {log += 'R'; return 'b';};
        a + b === 'ab' && log === 'GVSTR';
    "#,
    );
}

#[test]
fn invalid_exotic_methods_and_object_results_throw_type_error() {
    for exotic in ["42", "{}", "function() {return {};}"] {
        check(&format!(
            r#"
            var a = {{valueOf: function() {{throw 17;}}}};
            a[Symbol.toPrimitive] = {exotic};
            var caught = false;
            try {{a + 1;}} catch(e) {{caught = e.constructor === TypeError;}}
            caught;
        "#
        ));
    }
    for missing in ["undefined", "null"] {
        check(&format!(
            r#"
            var a = {{valueOf: function() {{return 2;}}}};
            a[Symbol.toPrimitive] = {missing}; a + 1 === 3;
        "#
        ));
    }
    check(
        r#"
        var a = {valueOf: function() {return {};}, toString: function() {return {};}};
        var caught = false;
        try {a + 1;} catch(e) {caught = e.constructor === TypeError;}
        caught;
    "#,
    );
    check(
        r#"
        var a = Object.create(null); var caught = false;
        try {a + 1;} catch(e) {caught = e.constructor === TypeError;}
        caught;
    "#,
    );
}

#[test]
fn bigint_and_symbol_results_keep_conversion_order_before_type_errors() {
    check(
        r#"
        var a = {valueOf: function() {return 2n;}};
        a + 3n === 5n && 3n + a === 5n && a + 'x' === '2x';
    "#,
    );
    for returned in ["2n", "Symbol.toPrimitive"] {
        check(&format!(
            r#"
            var log = ''; var a = {{}}; var b = {{}};
            a[Symbol.toPrimitive] = function() {{log += 'L'; return {returned};}};
            b[Symbol.toPrimitive] = function() {{log += 'R'; return 1;}};
            var caught = false;
            try {{a + b;}} catch(e) {{caught = e.constructor === TypeError;}}
            caught && log === 'LR';
        "#
        ));
    }
}

#[test]
fn compound_assignment_reads_once_and_writes_only_after_successful_conversion() {
    check(
        r#"
        var log = ''; var stored;
        var a = {valueOf: function() {log += 'L'; return 'a';}};
        var b = {valueOf: function() {log += 'R'; return 'b';}};
        var holder = {};
        Object.defineProperty(holder, 'value', {
            get: function() {log += 'G'; return a;},
            set: function(value) {log += 'S'; stored = value;}
        });
        function right() {log += 'E'; return b;}
        var result = holder.value += right();
        result === 'ab' && stored === 'ab' && log === 'GELRS';
    "#,
    );
    check(
        r#"
        var log = ''; var marker = {}; var caught = false;
        var a = {valueOf: function() {log += 'L'; throw marker;}};
        var holder = {};
        Object.defineProperty(holder, 'value', {
            get: function() {log += 'G'; return a;},
            set: function(value) {log += 'S';}
        });
        function right() {log += 'E'; return 1;}
        try {holder.value += right();} catch(e) {caught = e === marker;}
        caught && log === 'GEL';
    "#,
    );
}

#[test]
fn embedding_hooks_still_receive_the_default_hint_once() {
    use std::{cell::Cell, rc::Rc};
    use temiya::runtime::{Object, PrimitiveHint};
    let calls = Rc::new(Cell::new(0));
    let captured = calls.clone();
    let engine = Engine::default();
    let object = Object::new_in(engine.allocation_context());
    object.set_to_primitive_hook(move |hint| {
        assert_eq!(hint, PrimitiveHint::Default);
        captured.set(captured.get() + 1);
        Ok(Value::String("host".into()))
    });
    let realm = engine.create_realm();
    let handle = engine.value(&realm, Value::Object(object)).unwrap();
    engine.set_global(&realm, "host", handle).unwrap();
    let result = engine
        .evaluate(&engine.compile(&realm, "host + 'x' === 'hostx';").unwrap())
        .unwrap();
    assert!(matches!(result.value(), Value::Boolean(true)));
    assert_eq!(calls.get(), 1);
}

#[test]
#[ignore = "requires pinned Test262 corpus in TEST262_DIR"]
fn pinned_test262_addition_coercion() {
    use std::process::Command;
    use temiya::test262::{TEST262_REVISION, TestCase, TestVariant};
    let corpus = std::env::var("TEST262_DIR").expect("set TEST262_DIR");
    let selection = std::env::var("TEST262_CASE").unwrap_or_else(|_| {
        [
            "coerce-symbol-to-prim-return-prim.js",
            "coerce-symbol-to-prim-return-obj.js",
            "coerce-symbol-to-prim-err.js",
            "get-symbol-to-prim-err.js",
            "bigint-toprimitive.js",
            "coerce-bigint-to-string.js",
        ]
        .join(",")
    });
    let read = |path: &str| {
        let result = Command::new("git")
            .args(["-C", &corpus, "show", &format!("{TEST262_REVISION}:{path}")])
            .output()
            .expect("git show");
        assert!(result.status.success(), "missing pinned file {path}");
        String::from_utf8(result.stdout).unwrap()
    };
    let mut outcomes = Vec::new();
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    for name in selection.split(',') {
        assert!(name.ends_with(".js") && !name.contains(['/', '\\']) && !name.starts_with('.'));
        let path = format!("test/language/expressions/addition/{name}");
        let case = TestCase::parse(&path, read(&path)).unwrap();
        if case.metadata.negative.is_some()
            || case
                .metadata
                .flags
                .iter()
                .any(|f| f == "async" || f == "raw")
        {
            skipped += 1;
            outcomes.push(serde_json::json!({"path":path,"status":"skipped","reason":"unsupported metadata; not a pass"}));
            continue;
        }
        for variant in case.variants() {
            assert_ne!(variant, TestVariant::Module);
            let run = || -> Result<(), String> {
                let engine = Engine::default();
                let realm = engine.create_realm();
                for name in ["sta.js", "assert.js"]
                    .into_iter()
                    .chain(case.metadata.includes.iter().map(String::as_str))
                {
                    let script = engine
                        .compile(&realm, &read(&format!("harness/{name}")))
                        .map_err(|e| format!("harness {name} compile: {e}"))?;
                    engine
                        .evaluate(&script)
                        .map_err(|e| format!("harness {name} evaluate: {e:?}"))?;
                }
                let script = engine
                    .compile(&realm, &case.source_for(variant))
                    .map_err(|e| format!("compile: {e}"))?;
                engine.evaluate(&script).map_err(|e| {
                    let own_message = e.thrown().and_then(|handle| match handle.value() {
                        Value::Object(object) => object
                            .try_get_own_property(&"message".into())
                            .ok()
                            .flatten()
                            .and_then(|descriptor| match descriptor.value {
                                Some(Value::String(message)) => Some(message.to_string_lossy()),
                                _ => None,
                            }),
                        _ => None,
                    });
                    format!("evaluate: {e:?}; thrown own message: {own_message:?}")
                })?;
                Ok(())
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
        std::fs::write(path, serde_json::to_string_pretty(&serde_json::json!({"revision":TEST262_REVISION,"selected_files":selection.split(',').count(),"passed":passed,"failed":failed,"skipped_files":skipped,"outcomes":outcomes})).unwrap()).unwrap();
    }
    println!(
        "Test262 {TEST262_REVISION}: {passed} passed; {failed} failed; {skipped} skipped files"
    );
    assert!(
        passed > 0 && failed == 0 && skipped == 0,
        "selection is not fully passing"
    );
}
