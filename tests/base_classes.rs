use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        let result = engine
            .evaluate(&script)
            .unwrap_or_else(|error| panic!("{backend:?}: {error}: {source}"));
        assert!(
            matches!(result.value(), Value::Boolean(true)),
            "{backend:?}: {source}: {:?}",
            result.value()
        );
    }
}

#[test]
fn base_constructors_and_default_constructors_construct_real_instances() {
    check(
        "class Point { constructor(x) { this.x = x; } read() { return this.x; } } let p = new Point(42); p.read() === 42 && p instanceof Point && Object.getPrototypeOf(p) === Point.prototype;",
    );
    check(
        "let C = class Named {}; let a = new C(); let b = new C(); a !== b && a instanceof C && C.name === 'Named' && C.length === 0;",
    );
    check("class C { constructor() { return 3; } } new C() instanceof C;");
    check("let value = {}; class C { constructor() { return value; } } new C() === value;");
}

#[test]
fn inferred_class_names_and_constructor_new_target_are_observable() {
    check(
        "let C = class {}; var D = (class {}); let E; E = class {}; C.name === 'C' && D.name === 'D' && E.name === 'E';",
    );
    check("class C { constructor() { this.target = new.target; } } (new C()).target === C;");
    check(
        "let C = class Named { constructor() { this.target = new.target; } }; (new C()).target === C && C.name === 'Named';",
    );
}

#[test]
fn direct_class_calls_throw_before_running_the_body() {
    check(
        "let effects = 0; class C { constructor() { effects++; } } let caught = false; try { C(); } catch (e) { caught = e instanceof TypeError; } caught && effects === 0;",
    );
    check(
        "class C {} let caught = false; try { C.call({}); } catch (e) { caught = e instanceof TypeError; } caught;",
    );
}

#[test]
fn class_bindings_have_tdz_scope_and_distinct_inner_immutability() {
    check(
        "let caught = false; try { typeof C; } catch (e) { caught = e instanceof ReferenceError; } class C {} caught && typeof C === 'function' && !Object.prototype.hasOwnProperty.call(this, 'C');",
    );
    check("let C = 1; { class C {} } C === 1;");
    check(
        "class C { static self() { return C; } } let saved = C; C = 9; saved.self() === saved && C === 9;",
    );
    check(
        "let C = class Inner { static self() { return Inner; } }; C.self() === C && typeof Inner === 'undefined';",
    );
    check(
        "class C { static replace() { C = 1; } } let caught = false; try { C.replace(); } catch (e) { caught = e instanceof TypeError; } caught;",
    );
    check(
        "let caught = false; try { let C = class Inner { [Inner]() {} }; } catch (e) { caught = e instanceof ReferenceError; } caught;",
    );
}

#[test]
fn methods_are_strict_and_class_descriptors_are_non_enumerable() {
    check(
        "let caught = false; try { class C { static ['prototype']() {} } } catch (e) { caught = e instanceof TypeError; } caught;",
    );
    check(
        "class C { method() { return this; } static constructor() { return 7; } } let f = C.prototype.method; let d = Object.getOwnPropertyDescriptor(C, 'prototype'); let m = Object.getOwnPropertyDescriptor(C.prototype, 'method'); let c = Object.getOwnPropertyDescriptor(C.prototype, 'constructor'); f() === undefined && d.writable === false && d.enumerable === false && d.configurable === false && m.writable && !m.enumerable && m.configurable && c.value === C && !c.enumerable && C.constructor() === 7;",
    );
    check(
        "class C { constructor() { missingClassGlobal = 1; } } let caught = false; try { new C(); } catch (e) { caught = e instanceof ReferenceError; } caught && typeof missingClassGlobal === 'undefined';",
    );
}

#[test]
fn computed_names_accessors_and_static_methods_use_actual_class_objects() {
    check(
        "let calls = 0; function key() { calls++; return 'read'; } class C { constructor(x) { this.x = x; } [key()]() { return this.x; } get value() { return this.x; } set value(x) { this.x = x; } static make(x) { return new C(x); } } let p = C.make(2); p.value = 8; calls === 1 && p.read() === 8 && p.value === 8 && C.prototype.read.name === 'read' && Object.getOwnPropertyDescriptor(C.prototype, 'value').get.name === 'get value';",
    );
}

#[test]
fn class_environments_and_instances_survive_engine_collection() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, "let C = class Inner { constructor(x) { this.x = x; } read() { return this.x; } static self() { return Inner; } }; let instance = new C(23);").unwrap();
        engine.evaluate(&script).unwrap();
        engine.collect_garbage().unwrap();
        let script = engine
            .compile(
                &realm,
                "instance.read() === 23 && C.self() === C && instance instanceof C;",
            )
            .unwrap();
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
}
