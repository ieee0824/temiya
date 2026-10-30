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
            "{backend:?}: {:?}: {source}",
            result.value()
        );
    }
}

#[test]
fn derived_prototypes_and_new_target_follow_the_actual_constructor_chain() {
    check(
        "class A { constructor(x) { this.x = x; this.target = new.target; } read() { return this.x; } } class B extends A { constructor(x) { super(x); } } class C extends B {} let p = new C(42); p.read() === 42 && p.target === C && p instanceof A && p instanceof B && p instanceof C && Object.getPrototypeOf(B) === A && Object.getPrototypeOf(B.prototype) === A.prototype && Object.getPrototypeOf(C.prototype) === B.prototype;",
    );
}

#[test]
fn default_derived_constructor_forwards_every_argument() {
    check(
        "class A { constructor(a,b,c) { this.value = a + b + c; this.target = new.target; } } class B extends A {} let p = new B(10,20,12); p.value === 42 && p.target === B && B.length === 0;",
    );
}

#[test]
fn this_is_uninitialized_until_super_returns() {
    check(
        "let before = false; class A {} class B extends A { constructor() { try { this; } catch (e) { before = e instanceof ReferenceError; } super(); this.ok = 42; } } let p = new B(); before && p.ok === 42;",
    );
    check(
        "class A {} class B extends A { constructor() {} } let caught = false; try { new B(); } catch (e) { caught = e instanceof ReferenceError; } caught;",
    );
}

#[test]
fn duplicate_super_constructs_before_rejecting_the_second_this_binding() {
    check(
        "let calls = 0; class A { constructor() { calls++; } } class B extends A { constructor() { super(); super(); } } let caught = false; try { new B(); } catch (e) { caught = e instanceof ReferenceError; } caught && calls === 2;",
    );
}

#[test]
fn derived_returns_distinguish_objects_undefined_and_other_primitives() {
    check(
        "let replacement = {}; class A {} class B extends A { constructor() { return replacement; } } new B() === replacement;",
    );
    check(
        "class A {} class B extends A { constructor() { super(); return undefined; } } new B() instanceof B;",
    );
    for value in ["null", "0", "false", "'x'"] {
        check(&format!(
            "class A {{}} class B extends A {{ constructor() {{ return {value}; }} }} let caught = false; try {{ new B(); }} catch (e) {{ caught = e instanceof TypeError; }} caught;"
        ));
    }
}

#[test]
fn super_arguments_run_before_the_constructability_check() {
    check(
        "let effects = 0; class A {} class B extends A { constructor() { super(effects += 1); } } Object.setPrototypeOf(B, {}); let caught = false; try { new B(); } catch (e) { caught = e instanceof TypeError; } caught && effects === 1;",
    );
}

#[test]
fn super_uses_the_current_constructor_parent_and_preserves_new_target() {
    check(
        "class A { constructor() { this.value = 1; } } class Other { constructor() { this.value = 42; this.target = new.target; } } class B extends A { constructor() { super(); } } Object.setPrototypeOf(B, Other); let p = new B(); p.value === 42 && p.target === B && Object.getPrototypeOf(p) === B.prototype;",
    );
}

#[test]
fn arrows_created_before_super_share_the_later_initialized_this() {
    check(
        "let before = false; class A {} class B extends A { constructor() { let read = () => this; try { read(); } catch (e) { before = e instanceof ReferenceError; } super(); this.read = read; } } let p = new B(); before && p.read() === p;",
    );
}

#[test]
fn null_heritage_has_a_null_instance_prototype_and_no_default_super() {
    check(
        "class B extends null { constructor() { return Object.create(new.target.prototype); } } let p = new B(); Object.getPrototypeOf(B.prototype) === null && Object.getPrototypeOf(p) === B.prototype && Object.getPrototypeOf(B) === Function.prototype;",
    );
    check(
        "class B extends null {} let caught = false; try { new B(); } catch (e) { caught = e instanceof TypeError; } caught;",
    );
}

#[test]
fn heritage_checks_preserve_tdz_and_reject_invalid_prototypes() {
    check(
        "let caught = false; try { class C extends C {} } catch (e) { caught = e instanceof ReferenceError; } caught;",
    );
    check(
        "function A() {} A.prototype = 3; let effects = 0; let caught = false; try { class B extends A { [effects += 1]() {} } } catch (e) { caught = e instanceof TypeError; } caught && effects === 0;",
    );
    for value in ["3", "true", "'x'", "undefined", "({})", "(() => 1)"] {
        check(&format!(
            "let caught = false; try {{ class B extends {value} {{}} }} catch (e) {{ caught = e instanceof TypeError; }} caught;"
        ));
    }
}

#[test]
fn implicit_forwarding_does_not_read_the_array_iterator() {
    check(
        "let calls = 0; let marker = {}; Array.prototype[Symbol.iterator] = function() { calls++; throw marker; }; class A { constructor(a,b) { this.value = a + b; } } class B extends A {} let p = new B(20,22); let before = calls; class C extends A { constructor(a,b) { super(...[a,b]); } } let caught = false; try { new C(20,22); } catch (e) { caught = e === marker; } typeof Symbol.iterator === 'symbol' && p.value === 42 && before === 0 && calls === 1 && caught;",
    );
}

#[test]
fn super_results_and_parent_throws_keep_identity_and_run_once() {
    check(
        "let replacement = {}; class A { constructor() { return replacement; } } class B extends A { constructor() { let value = super(); if (value !== this) throw 1; this.ok = 42; } } new B() === replacement && replacement.ok === 42;",
    );
    check(
        "let calls = 0; let marker = {}; class A { constructor() { calls++; throw marker; } } class B extends A { constructor() { super(); } } let caught = false; try { new B(); } catch (e) { caught = e === marker; } caught && calls === 1;",
    );
    check(
        "let calls = 0; class A { constructor() { calls++; } } class B extends A { constructor() { super(); return 3; } } let caught = false; try { new B(); } catch (e) { caught = e instanceof TypeError; } caught && calls === 1;",
    );
}

#[test]
fn derived_errors_are_materialized_in_the_defining_realm() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let origin = engine.create_realm();
        let caller = engine.create_realm();
        let source = "class A {} class Early extends A { constructor() { this; } } class Primitive extends A { constructor() { return 1; } } class Twice extends A { constructor() { super(); super(); } }";
        let script = engine.compile(&origin, source).unwrap();
        engine.evaluate(&script).unwrap();
        for (name, error_type) in [
            ("Early", "ReferenceError"),
            ("Primitive", "TypeError"),
            ("Twice", "ReferenceError"),
        ] {
            let script = engine.compile(&origin, name).unwrap();
            let constructor = engine.evaluate(&script).unwrap();
            engine
                .set_global(
                    &caller,
                    name,
                    engine.value(&caller, constructor.value().clone()).unwrap(),
                )
                .unwrap();
            let script = engine
                .compile(&origin, &format!("{error_type}.prototype;"))
                .unwrap();
            let expected = engine.evaluate(&script).unwrap();
            let script = engine
                .compile(
                    &caller,
                    &format!("try {{ new {name}(); }} catch (e) {{ Object.getPrototypeOf(e); }}"),
                )
                .unwrap();
            let actual = engine.evaluate(&script).unwrap();
            assert!(
                actual.value().same_value(expected.value()),
                "{backend:?}: {name}"
            );
        }
    }
}

#[test]
fn derived_this_and_captured_arrows_survive_reentrant_collection() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        {
            let realm = engine.create_realm();
            let gc = temiya::runtime::Object::native_function_with_context(
                engine.allocation_context(),
                |context, _, _| {
                    context.collect_garbage()?;
                    Ok(Value::Undefined)
                },
            );
            engine
                .set_global(
                    &realm,
                    "gcNow",
                    engine.value(&realm, Value::Object(gc)).unwrap(),
                )
                .unwrap();
            let script = engine.compile(&realm, "class A { constructor(value) { gcNow(); this.value = value; } } class B extends A { constructor(value) { let read = () => this; super(value); gcNow(); this.read = read; } } let value = {}; let instance = new B(value);").unwrap();
            engine.evaluate(&script).unwrap();
            engine.collect_garbage().unwrap();
            let script = engine.compile(&realm, "instance.value === value && instance.read() === instance && instance instanceof B;").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn constructor_arguments_remain_available_to_arrows_defaults_and_strict_writes() {
    check(
        "class A { constructor(value) { this.value = value; } } class B extends A { constructor(value) { let read = () => arguments; value = 99; super(value); this.args = read(); } } let p = new B(42); p.value === 99 && p.args.length === 1 && p.args[0] === 42;",
    );
    check(
        "class A { constructor(value) { this.value = value; } } class B extends A { constructor(value = arguments.length) { super(value); } } new B().value === 0 && new B(undefined).value === 1;",
    );
    check(
        "class A {} class B extends A { constructor(value) { super(); this.count = arguments.length; this.extra = arguments[1]; } } let marker = {}; let p = new B(1,marker); p.count === 2 && p.extra === marker;",
    );
}
