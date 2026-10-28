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
fn utf16_indexing_preserves_surrogates_and_combines_only_valid_pairs() {
    check(
        r#"var s = 'A\uD83D\uDE00\uD800Z\uDC00';
        s.length === 6 && s.charAt(1) === '\uD83D' && s.at(-4) === '\uDE00' &&
        s.charCodeAt(1) === 55357 && s.codePointAt(1) === 128512 &&
        s.codePointAt(2) === 56832 && s.codePointAt(3) === 55296 &&
        s.codePointAt(5) === 56320 && s.codePointAt(0) === 65 &&
        '\uDBFF\uDFFF'.codePointAt(0) === 1114111 &&
        '\uD800'.codePointAt(0) === 55296 &&
        s.slice(1, 2) === '\uD83D' && s.substring(2, 1) === '\uD83D';"#,
    );
}

#[test]
fn positions_handle_missing_fractional_nan_infinite_negative_and_out_of_range() {
    for index in [
        "undefined",
        "NaN",
        "null",
        "false",
        "-0",
        "0.9",
        "-0.9",
        "'0'",
    ] {
        check(&format!(
            "'abc'.charAt({index}) === 'a' && 'abc'.at({index}) === 'a' &&
            'abc'.charCodeAt({index}) === 97 && 'abc'.codePointAt({index}) === 97;"
        ));
    }
    for index in ["-1", "3", "Infinity", "-Infinity", "9007199254740991"] {
        check(&format!(
            "'abc'.charAt({index}) === '' &&
            Number.isNaN('abc'.charCodeAt({index})) && 'abc'.codePointAt({index}) === undefined;"
        ));
    }
    check(
        "'abc'.at(-1.9) === 'c' && 'abc'.at(-3) === 'a' && 'abc'.at(-4) === undefined &&
        'abc'.at(Infinity) === undefined && 'abc'.at(-Infinity) === undefined &&
        ''.charAt() === '' && Number.isNaN(''.charCodeAt()) &&
        ''.at() === undefined && ''.codePointAt() === undefined;",
    );
}

#[test]
fn slice_and_substring_have_distinct_clamp_swap_and_default_rules() {
    check(
        "'abcdef'.slice(-4, -1) === 'cde' && 'abcdef'.slice(4, 2) === '' &&
        'abcdef'.substring(4, 2) === 'cd' && 'abcdef'.substring(-4, 2) === 'ab' &&
        'abcdef'.slice(-Infinity, Infinity) === 'abcdef' &&
        'abcdef'.substring(Infinity, -Infinity) === 'abcdef' &&
        'abcdef'.slice(1.9, 4.9) === 'bcd' && 'abcdef'.substring(NaN, 2) === 'ab' &&
        'abcdef'.slice() === 'abcdef' && 'abcdef'.substring(2, undefined) === 'cdef' &&
        'abcdef'.slice(2, null) === '' && 'abcdef'.substring(2, null) === 'ab';",
    );
}

#[test]
fn prototype_lookup_uses_original_primitive_receiver_and_canonical_indices() {
    check(
        "Number.prototype.slice = String.prototype.slice;
        Boolean.prototype.charAt = String.prototype.charAt;
        BigInt.prototype.substring = String.prototype.substring;
        (123).slice(1) === '23' && true.charAt(0) === 't' && (123n).substring(1) === '23';",
    );
    check("String.prototype['01'] = 'leading'; String.prototype['+1'] = 'plus';
        String.prototype['-0'] = 'minus'; String.prototype[9] = 'inherited';
        Object.defineProperty(String.prototype, 'probe', {get() {'use strict'; return this === 'abc';}});
        String.prototype[Symbol.toStringTag] = 'tag';
        'abc'.probe && 'abc'[1] === 'b' && 'abc'['01'] === 'leading' &&
        'abc'['+1'] === 'plus' && 'abc'['-0'] === 'minus' && 'abc'[9] === 'inherited' &&
        'abc'[Symbol.toStringTag] === 'tag';");
    check("var saved = String.prototype.charAt; String.prototype.charAt = function() { 'use strict'; return this; };
        var receiver = 'abc'.charAt(); delete String.prototype.charAt;
        receiver === 'abc' && 'abc'.charAt === undefined && saved.call('abc', 1) === 'b';");
}

#[test]
fn ordinary_string_properties_recheck_descriptors_and_preserve_virtual_keys() {
    let source = "var boxed=new String('ab');String.prototype.extra=41;\
        var a=boxed.extra===41;String.prototype.extra=42;var b=boxed.extra===42;\
        Object.defineProperty(String.prototype,'extra',{get:function(){'use strict';return this;},configurable:true});\
        var c=boxed.extra===boxed&&'ab'.extra==='ab';delete String.prototype.extra;\
        String.prototype[9]='far';String.prototype['01']='named';\
        boxed.extra===undefined&&a&&b&&c&&boxed.length===2&&boxed[0]==='a'&&\
        boxed[1]==='b'&&boxed[9]==='far'&&boxed['01']==='named';";
    for backend in [
        temiya::embedding::ScriptBackend::Ast,
        temiya::embedding::ScriptBackend::Bytecode,
    ] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert_eq!(script.backend(), backend);
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
}

#[test]
fn wrappers_and_borrowed_methods_observe_real_javascript_conversions() {
    check(
        "String() === '' && new String().valueOf() === '' && String(undefined) === 'undefined' &&
        String({toString() {return 'abc';}}).slice(1) === 'bc' &&
        new String({toString: null, valueOf() {return 'abc';}}).slice(1) === 'bc';",
    );
    check(
        "var caught = false; try { new String(Symbol.toPrimitive); }
        catch(e) { caught = e.constructor === TypeError; }
        caught && String(Symbol.toPrimitive).slice(0, 7) === 'Symbol(';",
    );
    check(
        "new String('abc').charAt(1) === 'b' && new String('abc').valueOf() === 'abc' &&
        String.prototype.valueOf() === '' && String.prototype.toString() === '' &&
        String.prototype.charAt.call(123, 1) === '2' &&
        String.prototype.slice.call(true, 1) === 'rue' &&
        String.prototype.substring.call(123n, 1) === '23' &&
        'abc'.charAt(new Number(1)) === 'b';",
    );
    check("var log = ''; var receiver = {toString() {log += 's'; return {};}, valueOf() {log += 'v'; return 'abcd';}};
        var start = {valueOf() {log += 'i'; return {};}, toString() {log += 'j'; return '1';}};
        var end = {[Symbol.toPrimitive](hint) {log += hint; return 3;}};
        var result = String.prototype.slice.call(receiver, start, end);
        result === 'bc' && log === 'svijnumber';");
    check(
        "var log = ''; var receiver = {[Symbol.toPrimitive](hint) { log += hint; return 'abc'; }};
        var end = {valueOf() {log += 'end'; return 0;}};
        String.prototype.slice.call(receiver, Infinity, end) === '' && log === 'stringend';",
    );
    check(
        "var s = new String('abc'); s.toString = function() {return 'XYZ';}; s.charAt(1) === 'Y';",
    );
}

#[test]
fn abrupt_coercions_preserve_identity_and_order() {
    for method in [
        "charAt",
        "charCodeAt",
        "codePointAt",
        "at",
        "slice",
        "substring",
    ] {
        for receiver in ["null", "undefined", "Symbol.toPrimitive"] {
            check(&format!("var ran = false; var caught = false;
                try {{ String.prototype.{method}.call({receiver}, {{valueOf() {{ran = true; return 0;}}}}); }}
                catch(e) {{caught = e.constructor === TypeError;}} caught && !ran;"));
        }
        for index in [
            "1n",
            "Symbol.toPrimitive",
            "{[Symbol.toPrimitive]: 1}",
            "{valueOf() {return {};}, toString() {return {};}}",
        ] {
            check(&format!(
                "var caught = false; try {{ 'abc'.{method}({index}); }}
                catch(e) {{caught = e.constructor === TypeError;}} caught;"
            ));
        }
        check(&format!(
            "var marker = {{}}; var caught = false;
            try {{ 'abc'.{method}({{get valueOf() {{throw marker;}}}}); }}
            catch(e) {{caught = e === marker;}} caught;"
        ));
    }
    check("var marker = {}; var caught = false; var receiver = new Proxy({}, {get(t, k) {throw marker;}});
        try { String.prototype.charAt.call(receiver); } catch(e) {caught = e === marker;} caught;");
    check(
        "var marker = {}; var caught = false; var count = 0;
        try { ''.slice({valueOf() {throw marker;}}, {valueOf() {count++; return 1;}}); }
        catch(e) {caught = e === marker;} caught && count === 0;",
    );
}

#[test]
fn method_descriptors_brand_checks_and_nonconstructability() {
    for (name, length) in [
        ("charAt", 1),
        ("charCodeAt", 1),
        ("codePointAt", 1),
        ("at", 1),
        ("slice", 2),
        ("substring", 2),
        ("toString", 0),
        ("valueOf", 0),
    ] {
        check(&format!("var f = String.prototype.{name}; var d = Object.getOwnPropertyDescriptor(String.prototype, '{name}');
            var n = Object.getOwnPropertyDescriptor(f, 'name'); var l = Object.getOwnPropertyDescriptor(f, 'length');
            var caught = false; try {{ new f(); }} catch(e) {{caught = e.constructor === TypeError;}}
            d.writable && !d.enumerable && d.configurable && n.value === '{name}' &&
            !n.writable && !n.enumerable && n.configurable && l.value === {length} &&
            !l.writable && !l.enumerable && l.configurable && f.prototype === undefined &&
            Object.getPrototypeOf(f) === Function.prototype && caught;"));
    }
    for name in ["toString", "valueOf"] {
        for receiver in ["{}", "42", "null", "new Proxy(new String('abc'), {})"] {
            check(&format!(
                "var caught = false; try {{ String.prototype.{name}.call({receiver}); }}
                catch(e) {{caught = e.constructor === TypeError;}} caught;"
            ));
        }
    }
}

#[test]
#[ignore = "requires pinned Test262 corpus; see docs/runtime.md"]
fn pinned_test262_string_methods() {
    let selection = std::env::var("TEST262_CASE").unwrap_or_else(|_| {
        [
            "at/index-argument-tointeger.js",
            "at/return-abrupt-from-this.js",
            "at/returns-code-unit.js",
            "at/returns-item-relative-index.js",
            "at/returns-item.js",
            "at/returns-undefined-for-out-of-range-index.js",
            "charAt/S9.4_A1.js",
            "charAt/S9.4_A2.js",
            "charAt/pos-coerce-err.js",
            "charAt/pos-coerce-string.js",
            "charAt/pos-rounding.js",
            "charAt/this-value-not-obj-coercible.js",
            "charCodeAt/pos-coerce-err.js",
            "charCodeAt/pos-coerce-string.js",
            "charCodeAt/pos-rounding.js",
            "charCodeAt/this-value-not-obj-coercible.js",
            "codePointAt/return-abrupt-from-object-pos-to-integer.js",
            "codePointAt/return-abrupt-from-this-as-symbol.js",
            "codePointAt/return-abrupt-from-this.js",
            "codePointAt/return-first-code-unit.js",
            "codePointAt/return-single-code-unit.js",
            "codePointAt/return-utf16-decode.js",
            "codePointAt/returns-undefined-on-position-equal-or-more-than-size.js",
            "codePointAt/returns-undefined-on-position-less-than-zero.js",
            "slice/this-value-not-obj-coercible.js",
            "substring/this-value-not-obj-coercible.js",
            "slice/S15.5.4.13_A1_T8.js",
            "substring/S15.5.4.15_A1_T9.js",
        ]
        .join(",")
    });
    run_pinned_string_cases(&selection);
}

fn run_pinned_string_cases(selection: &str) {
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
    for name in selection.split(',') {
        let path = format!("test/built-ins/String/prototype/{name}");
        let case = TestCase::parse(&path, read(&path)).unwrap();
        assert!(case.metadata.negative.is_none());
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
            for name in ["sta.js", "assert.js"]
                .into_iter()
                .chain(case.metadata.includes.iter().map(String::as_str))
            {
                let setup = engine
                    .compile(&realm, &read(&format!("harness/{name}")))
                    .unwrap_or_else(|e| panic!("harness {name} compile: {e}"));
                engine
                    .evaluate(&setup)
                    .unwrap_or_else(|e| panic!("harness {name} evaluate: {e:?}"));
            }
            let script = engine
                .compile(&realm, &case.source_for(variant))
                .unwrap_or_else(|e| panic!("{path} {variant:?} compile: {e}"));
            engine
                .evaluate(&script)
                .unwrap_or_else(|e| panic!("{path} {variant:?} evaluate: {e:?}"));
            passed += 1;
            println!("PASS {path} {variant:?}");
        }
    }
    println!("Test262 {TEST262_REVISION}: {passed} passed; 0 failed; 0 skipped in selection");
}
