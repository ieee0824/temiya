use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
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
fn preserves_parameter_order_missing_values_and_field_initialization() {
    check(
        "class C{first=41;constructor(a,b){this.x=b;this.y=a;this.x=a;}}let c=new C(42);c.first===41 && c.x===42 && c.y===42;",
    );
    check("class C{constructor(a,b){this.x=b;}}new C(42).x===undefined;");
}

#[test]
fn setters_receive_instance_and_abrupt_completion_stops_assignments() {
    check(
        "let log='';let seen;let marker={};class C{constructor(a,b){this.x=a;this.y=b;}set x(v){seen=this;log+=v;throw marker;}set y(v){log+='bad';}}let caught=false;try{new C(42,99);}catch(e){caught=e===marker;}caught && seen instanceof C && log==='42';",
    );
    check(
        "let seen;class C{first=41;constructor(a){this.x=a;}set x(v){seen=this;this.result=this.first+v;}}let c=new C(1);seen===c && c.result===42;",
    );
}

#[test]
fn preserves_strict_failure_and_default_parameter_fallback() {
    check(
        "class C{constructor(a){this.x=a;}}Object.defineProperty(C.prototype,'x',{value:0,writable:false});let caught=false;try{new C(42);}catch(e){caught=e instanceof TypeError;}caught;",
    );
    check(
        "let calls=0;class C{constructor(a=(calls++,42)){this.x=a;}}new C().x===42 && calls===1;",
    );
}

#[test]
fn alternate_new_target_and_derived_constructor_keep_their_semantics() {
    check(
        "class C{constructor(a){this.x=a;}}function N(){}let p={};N.prototype=p;let c=Reflect.construct(C,[42],N);Object.getPrototypeOf(c)===p && c.x===42;",
    );
    check(
        "class A{}class B extends A{constructor(a){this.x=a;}}let caught=false;try{new B(42);}catch(e){caught=e instanceof ReferenceError;}caught;",
    );
}
