use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        let result = engine
            .evaluate(&script)
            .unwrap_or_else(|e| panic!("{backend:?}: {e}; source={source}"));
        assert!(
            matches!(result.value(), Value::Boolean(true)),
            "{backend:?}: {:?}",
            result.value()
        );
    }
}

#[test]
fn methods_precede_fields_and_share_function_identity() {
    check(
        "class C{value=this.#m();#m(){return 42;}get(){return this.#m;}}let a=new C();let b=new C();a.value===42 && a.get()===b.get() && a.get().name==='#m' && Object.getOwnPropertyNames(a).length===1;",
    );
    check(
        "function make(){return class{#m(){}get(){return this.#m;}has(o){return #m in o;}};}let A=make();let B=make();let a=new A();let b=new B();a.get()!==b.get() && a.has(a) && !a.has(b);",
    );
}

#[test]
fn accessors_merge_halves_and_keep_receiver_and_names() {
    check(
        "class C{#x=0;get #v(){return this.#x;}set #v(v){this.#x=v;}put(v){this.#v=v;return this.#v;}has(o){return #v in o;}}let a=new C();let b=new C();a.put(42)===42 && b.put(41)===41 && a.has(a) && !a.has({});",
    );
    check(
        "class C{set #v(v){}get(){return this.#v;}}let caught=false;try{new C().get();}catch(e){caught=e instanceof TypeError;}caught;",
    );
    check(
        "class C{get #v(){return 42;}set(v){this.#v=v;}}let caught=false;try{new C().set(41);}catch(e){caught=e instanceof TypeError;}caught;",
    );
}

#[test]
fn private_methods_are_readonly_and_reject_proxy_or_inherited_brands() {
    check(
        "let side=0;class C{#m(){}set(){this.#m=(side++,42);}read(o){return o.#m;}has(o){return #m in o;}}let c=new C();let rejected=0;try{c.set();}catch(e){if(e instanceof TypeError)rejected++;}for(let o of [new Proxy(c,{}),Object.create(c),{},null]){try{c.read(o);}catch(e){if(e instanceof TypeError)rejected++;}}rejected===5 && side===1 && !c.has(new Proxy(c,{}));",
    );
}

#[test]
fn static_methods_precede_initializers_and_static_brands_do_not_inherit() {
    check(
        "class C{static #x=this.#m();static #m(){return 42;}static read(){return this.#x;}static has(o){return #m in o;}}class D extends C{}let rejected=false;try{D.read();}catch(e){rejected=e instanceof TypeError;}C.read()===42 && C.has(C) && !C.has(D) && rejected;",
    );
    check(
        "class C{static #x=0;static get #v(){return this.#x;}static set #v(v){this.#x=v;}static put(v){this.#v=v;return this.#v;}}C.put(42)===42 && !C.hasOwnProperty('#v');",
    );
}

#[test]
fn super_uses_private_method_home_object_and_accessor_errors_keep_identity() {
    check(
        "class A{value(){return this.input;}}class C extends A{input=42;#m(){return super.value();}read(){return this.#m();}}new C().read()===42;",
    );
    check(
        "class A{static get seed(){return this.input;}}class C extends A{static input=42;static #m(){return super.seed;}static read(){return this.#m();}}C.read()===42;",
    );
    check(
        "let marker={};let calls=0;class C{get #v(){calls++;throw marker;}read(){return this.#v;}}let caught=false;try{new C().read();}catch(e){caught=e===marker;}caught && calls===1;",
    );
}

#[test]
fn duplicate_method_stamping_fails_before_field_effects() {
    check(
        "let effects=0;class A{constructor(o){return o;}}class C extends A{#m(){}field=(effects++,42);static has(o){return #m in o;}}let o=Object.preventExtensions({});let fieldRejected=false;try{new C(o);}catch(e){fieldRejected=e instanceof TypeError;}let duplicate=false;try{new C(o);}catch(e){duplicate=e instanceof TypeError;}C.has(o) && effects===1 && fieldRejected && duplicate;",
    );
}

#[test]
fn private_method_and_accessor_edges_survive_reentrant_gc_then_release() {
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
            let script=engine.compile(&realm,"function make(){class C{#self=this;#x=41;#m(){gcNow();return this.#x+1;}get #v(){gcNow();return this.#m();}read(){return this.#v;}escape(){return this.#m;}}let c=new C();let fn=c.escape();gcNow();return c.read()===42 && fn.call(c)===42;}make();").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}
