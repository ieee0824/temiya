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
fn shared_function_code_never_shares_class_environments() {
    check(
        "function make(value) { return class Named { static read() { return value; } static self() { return Named; } }; } let A = make(1); let B = make(2); A !== B && A.read() === 1 && B.read() === 2 && A.self() === A && B.self() === B;",
    );
    check(
        "let total = 0; for (let i = 0; i < 20; i++) { let C = class { constructor(value) { this.value = value; } read() { return this.value; } }; total += new C(i).read(); } total === 190;",
    );
}

#[test]
fn compiled_and_fallback_class_bodies_never_replay_side_effects() {
    check(
        "let hits = 0; let marker = {}; class C { static fail(x) { hits += x; throw marker; } } let same = false; try { C.fail(1); } catch (e) { same = e === marker; } same && hits === 1;",
    );
    check(
        "let hits = 0; class C { static run(x = 1) { try { hits += x; return 7; } finally { hits += 2; } } } C.run() === 7 && hits === 3;",
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
fn non_strict_ordinary_function_legacy_properties_do_not_inherit_class_restrictions() {
    check(
        "function f() { return f.caller; } f() === null && f.arguments === null && f.hasOwnProperty('caller') && f.hasOwnProperty('arguments') && Object.getOwnPropertyDescriptor(f, 'caller').writable === false && Reflect.defineProperty(f, 'caller', {value: 1}) === false && delete f.caller === false;",
    );
    check(
        "function f() { 'use strict'; } let caught = false; try { f.caller; } catch (e) { caught = e instanceof TypeError; } caught && !f.hasOwnProperty('caller');",
    );
}

#[test]
fn class_restricted_properties_are_inherited_and_use_one_realm_thrower() {
    check(
        "class C {} let p = Object.getPrototypeOf(C); let a = Object.getOwnPropertyDescriptor(p, 'caller'); let b = Object.getOwnPropertyDescriptor(p, 'arguments'); let count = 0; try { C.caller; } catch (e) { if (e instanceof TypeError) count++; } try { C.arguments = 1; } catch (e) { if (e instanceof TypeError) count++; } count === 2 && !C.hasOwnProperty('caller') && !C.hasOwnProperty('arguments') && a.get === a.set && a.get === b.get && b.get === b.set && !a.enumerable && a.configurable && !Object.isExtensible(a.get) && a.get.name === '' && a.get.length === 0 && Object.getOwnPropertyDescriptor(a.get, 'name').configurable === false && Object.getOwnPropertyDescriptor(a.get, 'length').configurable === false;",
    );
}

#[test]
fn class_call_and_restricted_accessor_errors_belong_to_the_class_realm() {
    let engine = Engine::default();
    let origin = engine.create_realm();
    let caller = engine.create_realm();
    let definition = engine
        .compile(
            &origin,
            "class C { write(value) { this.value = value; return this.value; } } C;",
        )
        .unwrap();
    let class = engine.evaluate(&definition).unwrap();
    engine
        .set_global(
            &caller,
            "C",
            engine.value(&caller, class.value().clone()).unwrap(),
        )
        .unwrap();
    let prototype = engine.compile(&origin, "TypeError.prototype;").unwrap();
    let prototype = engine.evaluate(&prototype).unwrap();
    for source in [
        "try { C(); } catch (e) { Object.getPrototypeOf(e); }",
        "try { C.caller; } catch (e) { Object.getPrototypeOf(e); }",
        "try { C.prototype.write.call(Object.preventExtensions({}), 1); } catch (e) { Object.getPrototypeOf(e); }",
        "try { C.prototype.write.call(undefined, 1); } catch (e) { Object.getPrototypeOf(e); }",
    ] {
        let script = engine.compile(&caller, source).unwrap();
        let result = engine.evaluate(&script).unwrap();
        assert!(result.value().same_value(prototype.value()));
    }
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

#[test]
fn private_lexical_class_loops_preserve_shadowing_tdz_and_const() {
    check(
        "class C { static run(n) { let total = 0; for (let i = 0; i < n; i++) { for (let j = 0; j < i; j++) { total += j; } } return total; } } C.run(5) === 10 && C.run(-1) === 0 && C.run(NaN) === 0;",
    );
    check(
        "class C { static run(n) { let result = 0; for (let i = 0; i < n; i++) { let x; x++; result = x; } return result !== result; } } C.run(1);",
    );
    check(
        "class C { static run(n) { let sum = 0; for (let i = 0; i < n; i++) { const saved = i; { let i = saved + 1; sum += i; } } return sum; } } C.run(10) === 55 && C.run(0) === 0;",
    );
    check(
        "class C { static run(n) { let total = 0; for (let i = 0; i < n; i++) { let x; if (x !== undefined) return -1; x = i; total += x; } return total; } } C.run(5) === 10;",
    );
    check(
        "class C { static run(n) { for (let i = 0; i < n; i++) { value; let value = 1; } } } let caught = false; try { C.run(1); } catch (e) { caught = e instanceof ReferenceError; } caught;",
    );
    check(
        "class C { static run(n) { for (let i = 0; i < n; i++) { value = 1; let value; } } } let caught = false; try { C.run(1); } catch (e) { caught = e instanceof ReferenceError; } caught;",
    );
    check(
        "class C { static run(n) { for (let i = 0; i < n; i++) { const value = 1; value++; } } } let caught = false; try { C.run(1); } catch (e) { caught = e instanceof TypeError; } caught;",
    );
    check(
        "class C { static run(n) { for (let i = 0; i < n; i++) { const value = 1; value = 2; } } } let caught = false; try { C.run(1); } catch (e) { caught = e instanceof TypeError; } caught;",
    );
}

#[test]
fn observable_loop_bindings_and_object_coercions_keep_their_effects() {
    check(
        "class C { constructor(value) { this.value = value; } read() { let f = () => this.value; return f(); } } new C(42).read() === 42;",
    );
    check(
        "class C { static run(n) { let a; let b; for (let i = 0; i < n; i++) { let captured = i; if (i === 0) a = function() { return captured; }; else b = function() { return captured; }; } return a() === 0 && b() === 1; } } C.run(2);",
    );
    check(
        "let calls = 0; let input = { valueOf() { calls++; return 3; } }; class C { static run(n) { let total = 0; for (let i = 0; i < n; i++) { total += i; } return total; } } C.run(input) === 3 && calls === 4;",
    );
}

#[test]
fn this_lookup_crosses_declarative_blocks_and_hoisted_function_bodies() {
    check(
        "function FACTORY() { this.id = 0; this.id = func(); function func() { return 'id_string'; } } let obj; try { obj = new FACTORY(); } catch (e) { throw e; } obj.id === 'id_string';",
    );
    check(
        "class C { constructor() { this.value = 40; } read() { { let extra = 2; return this.value + extra; } } } new C().read() === 42;",
    );
    check(
        "class C { constructor() { this.value = 40; } read() { { let extra = 2; let f = () => this.value + extra; return f(); } } } new C().read() === 42;",
    );
}

#[test]
fn straight_line_methods_preserve_accessor_coercion_order_and_never_retry_errors() {
    check(
        "let log='';let current=1;let boxed={valueOf(){log+='v';return current;}};let target={get value(){log+='g';return boxed;},set value(x){log+='s';current=x;}};class C{add(x){this.value+=x;return this.value;}}let result=C.prototype.add.call(target,2);result===boxed && current===3 && log==='gvsg';",
    );
    check(
        "let log='';let marker={};let target={get value(){log+='g';return 1;},set value(x){log+='s';throw marker;}};class C{add(x){this.value+=x;return this.value;}}let same=false;try{C.prototype.add.call(target,2);}catch(e){same=e===marker;}same && log==='gs';",
    );
    check(
        "class C{write(value){this.value=value;return this.value;}bump(){return this.value++;}}let target={value:1n};C.prototype.write.call(target,4n)===4n && C.prototype.bump.call(target)===4n && target.value===5n;",
    );
    check(
        "class C{write(value){this.value=value;return this.value;}}let target={};let result=C.prototype.write.call(target);result===undefined && target.hasOwnProperty('value');",
    );
}

#[test]
fn noncapturing_callback_loops_keep_values_and_abrupt_control_flow() {
    check(
        "function make(value){return function(){return value;};}function run(n){let list=[];for(let i=0;i<n;i++){list.push(make(i));}return list[0]()+list[1]()+list[2]();}run(3)===3;",
    );
    check(
        "function run(n){let values=[];for(let i=0;i<n;i++){if(i===2)continue;if(i===5)break;values.push(i);}return values.length===4 && values[0]+values[1]+values[2]+values[3]===8;}run(10);",
    );
}
