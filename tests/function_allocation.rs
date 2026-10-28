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
fn default_prototype_is_stable_ordinary_data_with_constructor_backlink() {
    check(
        "var f = function named() {}; var p = f.prototype;
        var d = Object.getOwnPropertyDescriptor(f, 'prototype');
        var c = Object.getOwnPropertyDescriptor(p, 'constructor');
        p === f.prototype && d.value === p && d.writable && !d.enumerable && !d.configurable &&
        d.get === undefined && d.set === undefined && c.value === f && c.writable &&
        !c.enumerable && c.configurable && Object.getPrototypeOf(p) === Object.prototype &&
        Object.getPrototypeOf(new f()) === p && !(delete f.prototype);",
    );
}

#[test]
fn reflection_before_value_read_keeps_property_order_and_nonextensibility() {
    check("var f = function named() {}; f.tail = 1;
        var keys = Reflect.ownKeys(f);
        Object.preventExtensions(f);
        var p = f.prototype;
        keys[0] === 'length' && keys[1] === 'name' && keys[2] === 'prototype' && keys[3] === 'tail' &&
        p === Object.getOwnPropertyDescriptor(f, 'prototype').value &&
        p.constructor === f && Object.getPrototypeOf(new f()) === p && !Object.isExtensible(f);");
}

#[test]
fn bytecode_function_metadata_has_standard_descriptors_before_publication() {
    use temiya::embedding::ScriptBackend;
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Bytecode)
        .build();
    let realm = engine.create_realm();
    let script = engine.compile(&realm, "var f=function named(a,b){};var l=Object.getOwnPropertyDescriptor(f,'length');var n=Object.getOwnPropertyDescriptor(f,'name');var arrow=(x)=>x;var a=Object.getOwnPropertyDescriptor(arrow,'name');l.value===2&&!l.writable&&!l.enumerable&&l.configurable&&n.value==='named'&&!n.writable&&!n.enumerable&&n.configurable&&typeof a.value==='string'&&!a.writable&&!a.enumerable&&a.configurable&&!Object.prototype.hasOwnProperty.call(arrow,'prototype')&&Object.defineProperty(f,'name',{value:'renamed'}).name==='renamed';").unwrap();
    assert_eq!(script.backend(), ScriptBackend::Bytecode);
    assert!(
        engine
            .evaluate(&script)
            .unwrap()
            .value()
            .same_value(&Value::Boolean(true))
    );
}

#[test]
fn prototype_redefinition_preserves_descriptor_invariants() {
    check(
        "var f = function() {}; var replacement = {};
        Object.defineProperty(f, 'prototype', {value: replacement, writable: false});
        var same = Reflect.defineProperty(f, 'prototype', {value: replacement});
        var changed = Reflect.defineProperty(f, 'prototype', {value: undefined});
        var accessor = Reflect.defineProperty(f, 'prototype', {get() {return 1;}});
        var configurable = Reflect.defineProperty(f, 'prototype', {configurable: true});
        same && !changed && !accessor && !configurable && f.prototype === replacement &&
        Object.getPrototypeOf(new f()) === replacement;",
    );
    check(
        "var f = function() {}; Object.defineProperty(f, 'prototype', {writable:false});
        var p = f.prototype; p.constructor === f &&
        Reflect.defineProperty(f, 'prototype', {value:p}) &&
        !Reflect.defineProperty(f, 'prototype', {value:undefined});",
    );
    check(
        "var f = function() {}; f.prototype = 7;
        var d = Object.getOwnPropertyDescriptor(f, 'prototype');
        d.value === 7 && d.writable && !d.enumerable && !d.configurable;",
    );
}

#[test]
fn proxy_invariants_and_getters_observe_the_real_prototype_value() {
    check(
        "var f = function() {}; var proxy = new Proxy(f, {});
        var p = proxy.prototype;
        Object.getOwnPropertyDescriptor(proxy, 'prototype').value === p && p.constructor === f &&
        Object.getPrototypeOf(new proxy()) === p;",
    );
    check("var f = function() {}; var proxy = new Proxy(f, {ownKeys() {return ['length'];}});
        var caught = false; try { Reflect.ownKeys(proxy); } catch(e) {caught = e.constructor === TypeError;} caught;");
    check("var f = function() {}; var p = f.prototype;
        Object.defineProperty(f, 'prototype', {writable:false});
        var proxy = new Proxy(f, {get() {return {};}}); var caught=false;
        try {proxy.prototype;} catch(e) {caught = e.constructor === TypeError;} caught && f.prototype === p;");
}

#[test]
fn active_anonymous_functions_survive_removal_of_their_last_global_reference() {
    check(
        "var saved = function(n) {saved = null; return n + 1;}; saved(41) === 42 && saved === null;",
    );
    check("var C = function() { C = null; this.answer = 42; }; var instance = new C();
        instance.answer === 42 && C === null && Object.getPrototypeOf(instance).constructor !== undefined;");
    check(
        "var f = () => 42; var o = {method() {return 42;}};
        f.prototype === undefined && o.method.prototype === undefined;",
    );
}

#[path = "../examples/support/ownership.rs"]
mod ownership;

#[test]
fn unobserved_function_prototypes_and_hidden_self_slots_do_not_pin_host_values() {
    for name in [
        "acyclic-object",
        "arrow-function",
        "ordinary-function",
        "lexical-closure",
    ] {
        let case = ownership::CASES
            .iter()
            .find(|case| case.name == name)
            .unwrap();
        let result = ownership::observe(case).unwrap();
        assert_eq!(result.name, name);
        assert!(result.alive_with_handle && result.alive_with_clone);
        assert!(
            !result.alive_after_drop,
            "unreachable function retained host value: {result:?}"
        );
    }
    // OwnKeys observes the data property's name, not its object value.
    let case = ownership::Case {
        name: "enumerated-function",
        source: "var value = function() {return 42;}; value.probe = marker; Reflect.ownKeys(value); delete marker; value;",
        constructor_root: false,
        callable_root: true,
    };
    assert!(!ownership::observe(&case).unwrap().alive_after_drop);
}

#[test]
#[ignore = "requires pinned Test262 corpus; see docs/runtime-ownership.md"]
fn pinned_test262_function_prototypes() {
    let selection = std::env::var("TEST262_CASE")
        .unwrap_or_else(|_| "S13.2_A1_T1.js,S13.2_A1_T2.js,S13.2_A4_T1.js,S13.2_A4_T2.js".into());
    run_pinned_function_cases(&selection);
}

fn run_pinned_function_cases(selection: &str) {
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
        let path = format!("test/language/statements/function/{name}");
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
