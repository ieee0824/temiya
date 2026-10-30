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
            let script = engine.compile(&realm, "class A { constructor(value) { gcNow(); this.value = value; } } class B extends A { constructor(value) { let read = () => this; super(value); gcNow(); this.read = read; } } class D extends A { constructor(value) { super(value); } } let delegated = new D({}); let value = {}; let instance = new B(value);").unwrap();
            engine.evaluate(&script).unwrap();
            engine.collect_garbage().unwrap();
            let script = engine.compile(&realm, "instance.value === value && instance.read() === instance && instance instanceof B && typeof delegated.value === 'object' && Object.getPrototypeOf(delegated) === D.prototype;").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
            let script = engine.compile(&realm, "function factory(captured) { return class extends A { constructor(value) { super(value); gcNow(); } read() { gcNow(); return captured; } }; } let captured = {}; let Anonymous = factory(captured); let anonymous = new Anonymous(captured); gcNow(); anonymous.read() === captured && anonymous.value === captured && anonymous instanceof Anonymous;").unwrap();
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

#[test]
fn object_subclasses_allocate_with_new_target_and_ignore_conversion_arguments() {
    check(
        "class Obj extends Object { valueOf() { return 42; } } let input = {}; let p = new Obj(input); p !== input && p.valueOf() === 42 && p instanceof Obj && Object.getPrototypeOf(p) === Obj.prototype && Object(input) === input && new Object(input) === input;",
    );
}

#[test]
fn non_object_prototype_uses_the_new_targets_realm_through_wrappers() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let origin = engine.create_realm();
        let caller = engine.create_realm();
        let script = engine
            .compile(
                &origin,
                "function Foreign() {} Foreign.prototype = null; Foreign;",
            )
            .unwrap();
        let foreign = engine.evaluate(&script).unwrap();
        let Value::Object(target) = foreign.value() else {
            panic!("constructor")
        };
        let handler = temiya::runtime::Object::new_in(engine.allocation_context());
        let proxy = temiya::runtime::ProxyObject::try_new(target.clone(), handler)
            .unwrap()
            .object();
        for (name, object) in [("Target", target.clone()), ("ProxyTarget", proxy)] {
            engine
                .set_global(
                    &caller,
                    name,
                    engine.value(&caller, Value::Object(object)).unwrap(),
                )
                .unwrap();
        }
        let script = engine.compile(&caller, "Target.bind(null);").unwrap();
        let bound = engine.evaluate(&script).unwrap();
        engine
            .set_global(
                &caller,
                "BoundTarget",
                engine.value(&caller, bound.value().clone()).unwrap(),
            )
            .unwrap();
        let script = engine.compile(&origin, "Object.prototype;").unwrap();
        let expected = engine.evaluate(&script).unwrap();
        for name in ["Target", "ProxyTarget", "BoundTarget"] {
            for body in [
                "let Base = function() { this.target = new.target; };",
                "class Parent { constructor() { this.target = new.target; } } let Base = class extends Parent { constructor() { super(); } };",
            ] {
                let source = format!(
                    "{{ {body} let p = Reflect.construct(Base, [], {name}); p.target === {name} && Object.getPrototypeOf(p); }}"
                );
                let script = engine.compile(&caller, &source).unwrap();
                let actual = engine.evaluate(&script).unwrap();
                assert!(
                    actual.value().same_value(expected.value()),
                    "{backend:?}: {name}: {body}"
                );
            }
        }
    }
}

#[test]
fn prototype_getter_revocation_is_checked_only_when_realm_fallback_is_needed() {
    check(
        "let pair; function Target() {} function Base() {} pair = Proxy.revocable(Target, { get(t,key) { if (key === 'prototype') { pair.revoke(); return null; } return t[key]; } }); let caught = false; try { Reflect.construct(Base, [], pair.proxy); } catch (e) { caught = e instanceof TypeError; } caught;",
    );
    check(
        "let pair; let proto = {}; function Target() {} function Base() {} pair = Proxy.revocable(Target, { get(t,key) { if (key === 'prototype') { pair.revoke(); return proto; } return t[key]; } }); let p = Reflect.construct(Base, [], pair.proxy); Object.getPrototypeOf(p) === proto;",
    );
}

#[test]
fn allocation_errors_before_entering_a_foreign_constructor_use_the_caller_realm() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let origin = engine.create_realm();
        let caller = engine.create_realm();
        for (name, source) in [
            ("Foreign", "class Foreign {} Foreign;"),
            ("ForeignPlain", "(function() {});"),
            (
                "ForeignBody",
                "class ForeignBody { constructor() { null.x; } } ForeignBody;",
            ),
            ("ForeignPlainBody", "(function() { null.x; });"),
        ] {
            let script = engine.compile(&origin, source).unwrap();
            let constructor = engine.evaluate(&script).unwrap();
            engine
                .set_global(
                    &caller,
                    name,
                    engine.value(&caller, constructor.value().clone()).unwrap(),
                )
                .unwrap();
        }
        let script = engine.compile(&caller, "TypeError.prototype;").unwrap();
        let expected_caller = engine.evaluate(&script).unwrap();
        let script = engine.compile(&origin, "TypeError.prototype;").unwrap();
        let expected_body = engine.evaluate(&script).unwrap();
        for constructor in ["Foreign", "ForeignPlain"] {
            for wrapper in [
                constructor.to_string(),
                format!("{constructor}.bind(null)"),
                format!("Proxy.revocable({constructor}, {{}}).proxy"),
            ] {
                let source = format!(
                    "{{ let pair; function Target() {{}} pair = Proxy.revocable(Target, {{ get(t,key) {{ if (key === 'prototype') {{ pair.revoke(); return null; }} return t[key]; }} }}); try {{ Reflect.construct({wrapper}, [], pair.proxy); }} catch (e) {{ Object.getPrototypeOf(e); }} }}"
                );
                let script = engine.compile(&caller, &source).unwrap();
                let actual = engine.evaluate(&script).unwrap();
                assert!(
                    actual.value().same_value(expected_caller.value()),
                    "{backend:?}: {wrapper}"
                );
            }
        }
        for constructor in ["ForeignBody", "ForeignPlainBody"] {
            let script = engine
                .compile(
                    &caller,
                    &format!(
                        "try {{ new {constructor}(); }} catch (e) {{ Object.getPrototypeOf(e); }}"
                    ),
                )
                .unwrap();
            let actual = engine.evaluate(&script).unwrap();
            assert!(
                actual.value().same_value(expected_body.value()),
                "{backend:?}: {constructor}"
            );
        }
    }
}

#[test]
fn native_array_number_and_string_subclasses_keep_prototypes_and_payloads() {
    check(
        "class A extends Array {} let p = new A(1,2); p instanceof A && Object.getPrototypeOf(p) === A.prototype && Array.isArray(p) && p.length === 2 && p[1] === 2;",
    );
    check(
        "class N extends Number {} let p = new N(42); p instanceof N && Object.getPrototypeOf(p) === N.prototype && Number.prototype.valueOf.call(p) === 42;",
    );
    check(
        "class S extends String {} let p = new S('abc'); p instanceof S && Object.getPrototypeOf(p) === S.prototype && String.prototype.valueOf.call(p) === 'abc' && p.length === 3 && p[1] === 'b';",
    );
}

#[test]
fn native_collection_buffer_date_and_regexp_subclasses_keep_their_slots() {
    check(
        "class M extends Map {} let p = new M(); p.set('key',42); p instanceof M && Object.getPrototypeOf(p) === M.prototype && p.get('key') === 42;",
    );
    check("class S extends Set {} let p = new S(); p.add(42); p instanceof S && p.has(42);");
    check(
        "class W extends WeakMap {} let p = new W(); let key = {}; p.set(key,42); p instanceof W && p.get(key) === 42;",
    );
    check(
        "class W extends WeakSet {} let p = new W(); let key = {}; p.add(key); p instanceof W && p.has(key);",
    );
    check(
        "class B extends ArrayBuffer {} let p = new B(4); p instanceof B && Object.getPrototypeOf(p) === B.prototype && p.byteLength === 4;",
    );
    check(
        "class D extends Date {} let p = new D(0); p instanceof D && Object.getPrototypeOf(p) === D.prototype && Date.prototype.valueOf.call(p) === 0;",
    );
    check(
        "class R extends RegExp {} let p = new R('a'); p instanceof R && Object.getPrototypeOf(p) === R.prototype && p.exec('a')[0] === 'a';",
    );
}

#[test]
fn default_intrinsic_factories_use_the_derived_prototype() {
    check(
        "class E extends Error { own() { return 42; } } let p = new E(); Object.getPrototypeOf(p) === E.prototype && p.own() === 42;",
    );
    check(
        "class B extends Boolean { own() { return 42; } } let p = new B(); Object.getPrototypeOf(p) === B.prototype && p.own() === 42;",
    );
}

#[test]
fn native_conversion_and_prototype_getters_keep_their_required_order() {
    check(
        "let log = ''; let proto = {}; function Target() {} let target = Proxy.revocable(Target, { get(t,key) { if (key === 'prototype') { log += 'p'; return proto; } return t[key]; } }).proxy; let value = { valueOf() { log += 'v'; return 42; } }; let p = Reflect.construct(Number, [value], target); log === 'vp' && Object.getPrototypeOf(p) === proto && Number.prototype.valueOf.call(p) === 42;",
    );
    check(
        "let log = ''; let proto = {}; function Target() {} let target = Proxy.revocable(Target, { get(t,key) { if (key === 'prototype') { log += 'p'; return proto; } return t[key]; } }).proxy; let value = { toString() { log += 's'; return 'abc'; } }; let p = Reflect.construct(String, [value], target); log === 'sp' && Object.getPrototypeOf(p) === proto && String.prototype.valueOf.call(p) === 'abc';",
    );
    check(
        "let log = ''; function Target() {} let target = Proxy.revocable(Target, { get(t,key) { if (key === 'prototype') { log += 'p'; return {}; } return t[key]; } }).proxy; let caught = false; try { Reflect.construct(Array, [1.5], target); } catch (e) { caught = e instanceof RangeError; } caught && log === 'p';",
    );
}

#[test]
fn native_intrinsic_fallbacks_come_from_the_new_targets_realm() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let origin = engine.create_realm();
        let caller = engine.create_realm();
        let script = engine.compile(&origin, "(function() {});").unwrap();
        let target = engine.evaluate(&script).unwrap();
        engine
            .set_global(
                &caller,
                "Target",
                engine.value(&caller, target.value().clone()).unwrap(),
            )
            .unwrap();
        let script = engine.compile(&caller, "Target.prototype = null;").unwrap();
        engine.evaluate(&script).unwrap();
        for (name, args) in [
            ("Array", "[1,2]"),
            ("Number", "[42]"),
            ("String", "['abc']"),
            ("Map", "[]"),
            ("ArrayBuffer", "[4]"),
            ("Date", "[0]"),
            ("RegExp", "['a']"),
            ("Error", "[]"),
        ] {
            let script = engine
                .compile(&origin, &format!("{name}.prototype;"))
                .unwrap();
            let expected = engine.evaluate(&script).unwrap();
            let script = engine
                .compile(
                    &caller,
                    &format!("Object.getPrototypeOf(Reflect.construct({name}, {args}, Target));"),
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
fn derived_data_views_keep_their_buffer_and_recheck_detachment_after_getters() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, "class V extends DataView {} let buffer = new ArrayBuffer(8); let view = new V(buffer,2,4); view;").unwrap();
        let value = engine.evaluate(&script).unwrap();
        let Value::Object(object) = value.value() else {
            panic!("view")
        };
        let view = temiya::runtime::TypedArray::from_object(object.clone());
        // DataView shares the runtime's checked byte-view storage.
        let view = view.expect("derived DataView payload");
        let script = engine
            .compile(
                &realm,
                "buffer = null; Object.getPrototypeOf(view) === V.prototype;",
            )
            .unwrap();
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
        engine.collect_garbage().unwrap();
        view.set(0, 42).unwrap();
        assert_eq!(view.length().unwrap(), 4);
        assert!(view.get(0).unwrap().same_value(&Value::Number(42.0)));
        let detach = temiya::runtime::Object::native_function_with_context(
            engine.allocation_context(),
            |_, _, args| {
                let Some(Value::Object(object)) = args.first() else {
                    return Err(temiya::runtime::RuntimeError::type_error("buffer required"));
                };
                let buffer = temiya::runtime::ArrayBuffer::from_object(object.clone())
                    .ok_or_else(|| temiya::runtime::RuntimeError::type_error("buffer required"))?;
                buffer.detach();
                Ok(Value::Undefined)
            },
        );
        engine
            .set_global(
                &realm,
                "detach",
                engine.value(&realm, Value::Object(detach)).unwrap(),
            )
            .unwrap();
        let script = engine
            .compile(&realm, "new DataView(new ArrayBuffer(8), 2.9, undefined);")
            .unwrap();
        let value = engine.evaluate(&script).unwrap();
        let Value::Object(object) = value.value() else {
            panic!("view")
        };
        assert_eq!(
            temiya::runtime::TypedArray::from_object(object.clone())
                .unwrap()
                .length()
                .unwrap(),
            6
        );
        let source = "function invalid(buffer, offset, length, kind) { let reads = 0; function Target() {} let target = Proxy.revocable(Target, { get(t,key) { if (key === 'prototype') { reads++; return {}; } return t[key]; } }).proxy; let caught = false; try { Reflect.construct(DataView, [buffer,offset,length], target); } catch(e) { caught = e instanceof kind; } return caught && reads === 0; } let old = new ArrayBuffer(8); detach(old); invalid(new ArrayBuffer(8),9,undefined,RangeError) && invalid(new ArrayBuffer(8),2,7,RangeError) && invalid(old,0,undefined,TypeError) && invalid(old,4294967296,undefined,TypeError) && invalid(old,9007199254740991,undefined,TypeError) && invalid(old,9007199254740992,undefined,RangeError) && invalid(new ArrayBuffer(8),Infinity,0,RangeError) && invalid(new ArrayBuffer(8),-1,0,RangeError);";
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}: validation must precede prototype access"
        );
        let source = "let detachedBuffer = new ArrayBuffer(8); function Target() {} let target = Proxy.revocable(Target, { get(t,key) { if (key === 'prototype') { detach(detachedBuffer); return {}; } return t[key]; } }).proxy; let caught = false; try { Reflect.construct(DataView, [detachedBuffer], target); } catch (e) { caught = e instanceof TypeError ? true : e.name + ': ' + e.message; } caught;";
        let script = engine.compile(&realm, source).unwrap();
        let result = engine.evaluate(&script).unwrap();
        assert!(
            matches!(result.value(), Value::Boolean(true)),
            "{backend:?}: {:?}",
            result.value()
        );
    }
}

#[test]
fn sole_super_preserves_explicit_argument_mapping_and_original_new_target() {
    check(
        "class A { constructor(a,b) { this.a=a; this.b=b; this.count=arguments.length; } } class D extends A { constructor(x,y) { super(x,y); } } let p=new D(42); p.a===42 && p.b===undefined && p.count===2;",
    );
    check(
        "class A { constructor(a) { this.a=a; this.count=arguments.length; } } class D extends A { constructor(x) { super(x); } } let p=new D(42,99); p.a===42 && p.count===1;",
    );
    check(
        "class A { constructor(a,b,c) { this.a=a; this.b=b; this.c=c; this.count=arguments.length; this.target=new.target; } } class D extends A { constructor(x,y,z) { super(y,x,y); } } let marker={}; let p=new D(42,marker,99,100); p.a===marker && p.b===42 && p.c===marker && p.count===3 && p.target===D && Object.getPrototypeOf(p)===D.prototype;",
    );
    check(
        "class A { constructor(a,b) { this.a=a; this.b=b; this.count=arguments.length; } } class D extends A { constructor(x,y) { super(y,x); } } let p=new D(42); p.a===undefined && p.b===42 && p.count===2;",
    );
    check(
        "class A { constructor() { this.count=arguments.length; } } class D extends A { constructor(x) { super(); } } new D(42,99).count===0;",
    );
    check(
        "class A {} class Other { constructor(x) { this.x=x; this.target=new.target; } } class D extends A { constructor(x) { super(x); } } Object.setPrototypeOf(D,Other); let p=new D(42); p.x===42 && p.target===D && Object.getPrototypeOf(p)===D.prototype;",
    );
    check(
        "let marker={}; class A { constructor(value) { return value; } } class D extends A { constructor(value) { super(value); } } new D(marker)===marker;",
    );
    check(
        "let calls=0; function mark(v) { calls++; return v; } class A { constructor(v) { this.v=v; } } class D extends A { constructor(v=mark(42)) { super(mark(v)); } } let p=new D(); p.v===42 && calls===2;",
    );
}

#[test]
fn anonymous_classes_preserve_outer_bindings_strict_this_and_inferred_names() {
    check(
        "let C=class { static read(){return C;} }; let original=C; C=42; original.read()===42 && original.name==='C';",
    );
    check(
        "let C=class Inner { static read(){return Inner;} }; let original=C; C=42; original.read()===original && original.name==='Inner';",
    );
    check(
        "function make(value){return class { constructor(){this.value=value;} read(){return value;} };} let C=make(42); let p=new C(); p.value===42 && p.read()===42;",
    );
    check(
        "let marker={key:'read'}; function make(value){ return class extends value { [this.key](){return this.value;} }; } class A { constructor(v){this.value=v;} } let C=make.call(marker,A); let p=new C(42); p.read()===42 && Object.getPrototypeOf(C)===A;",
    );
    check(
        "let C=class { read(){return this;} }; let method=C.prototype.read; method()===undefined;",
    );
    check(
        "let Outer=class {}; let caught=false; try { let Outer=class extends Outer {}; } catch(e){caught=e instanceof ReferenceError;} caught;",
    );
}

#[test]
fn fresh_class_function_metadata_preserves_descriptors_and_key_order() {
    check(
        "function metadata(fn,name,length){let n=Object.getOwnPropertyDescriptor(fn,'name');let l=Object.getOwnPropertyDescriptor(fn,'length');return n.value===name && l.value===length && n.writable===false && l.writable===false && n.enumerable===false && l.enumerable===false && n.configurable===true && l.configurable===true;}class C{constructor(a,b=0,c){}read(a,b=0,c){}get value(){return 42;}set value(v){}static make(a,...rest){}[Symbol.iterator](...rest){}}let value=Object.getOwnPropertyDescriptor(C.prototype,'value');metadata(C,'C',1) && metadata(C.prototype.read,'read',1) && metadata(value.get,'get value',0) && metadata(value.set,'set value',1) && metadata(C.make,'make',1) && metadata(C.prototype[Symbol.iterator],'[Symbol.iterator]',0);",
    );
    check(
        "class A{}class B extends A{}let keys=Reflect.ownKeys(B);keys.length===3 && keys[0]==='length' && keys[1]==='name' && keys[2]==='prototype' && B.length===0 && B.name==='B' && Object.getOwnPropertyDescriptor(B,'prototype').writable===false;",
    );
    check(
        "class C{read(){}}let keys=Reflect.ownKeys(C.prototype.read);keys.length===2 && keys[0]==='length' && keys[1]==='name' && !C.prototype.read.hasOwnProperty('prototype');",
    );
}
