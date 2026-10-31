use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        let result = engine
            .evaluate(&script)
            .unwrap_or_else(|e| panic!("{backend:?}: {e}"));
        assert!(
            matches!(result.value(), Value::Boolean(true)),
            "{backend:?}: {:?}",
            result.value()
        );
    }
}

#[test]
fn private_fields_read_write_update_and_call_with_receiver() {
    check(
        "class C{#x=40;#fn=function(){return this;};read(){return this.#x;}bump(){this.#x++;return this.#x+=1;}self(){return this.#fn();}}let c=new C();c.read()===40 && c.bump()===42 && c.self()===c && Object.getOwnPropertyNames(c).length===0;",
    );
}

#[test]
fn brands_do_not_follow_prototypes_or_proxy_targets() {
    check(
        "class C{#x=42;has(o){return #x in o;}read(o){return o.#x;}}let c=new C();let p=new Proxy(c,{});let f=Object.create(c);let rejected=0;for(let o of [p,f,{},null,42]){try{c.read(o);}catch(e){if(e instanceof TypeError)rejected++;}}c.has(c) && !c.has(p) && !c.has(f) && rejected===5;",
    );
}

#[test]
fn repeated_class_evaluation_and_nested_shadowing_have_distinct_names() {
    check(
        "function make(){return class{#x=42;read(o){return o.#x;}};}let A=make();let B=make();let a=new A();let b=new B();let caught=false;try{a.read(b);}catch(e){caught=e instanceof TypeError;}caught && a.read(a)===42;",
    );
    check(
        "class A{#x=41;nested(){return class B{#x=42;read(o){return o.#x;}};}reader(){return o=>o.#x;}}let a=new A();let B=a.nested();let b=new B();let caught=false;try{a.reader()(b);}catch(e){caught=e instanceof TypeError;}caught && a.reader()(a)===41 && b.read(b)===42;",
    );
}

#[test]
fn initialization_order_and_partial_brands_follow_field_definition() {
    check(
        "let log='';class C{#a=(log+='a',1);b=(log+='b',this.#a+1);#c=(log+='c',this.b+40);read(){return this.#c;}has(o){return #a in o;}}let c=new C();log==='abc' && c.read()===42 && c.has(c);",
    );
    check(
        "let escaped;class C{first=(escaped=this);#a=this.#b;#b=42;hasA(o){return #a in o;}hasB(o){return #b in o;}}let caught=false;try{new C();}catch(e){caught=e instanceof TypeError;}caught && !C.prototype.hasA(escaped) && !C.prototype.hasB(escaped);",
    );
}

#[test]
fn stamping_nonextensible_receiver_and_duplicate_initialization() {
    check(
        "class Base{constructor(o){return o;}}class Stamp extends Base{#x=42;static read(o){return o.#x;}static has(o){return #x in o;}}let o=Object.preventExtensions({});new Stamp(o);let duplicate=false;try{new Stamp(o);}catch(e){duplicate=e instanceof TypeError;}Stamp.has(o) && Stamp.read(o)===42 && duplicate;",
    );
}

#[test]
fn assignment_checks_brand_after_rhs_and_optional_access_short_circuits() {
    check(
        "let calls=0;class C{#x;set(o){o.#x=(calls++,42);}optional(o){return o?.#x;}}let c=new C();let caught=false;try{c.set({});}catch(e){caught=e instanceof TypeError;}caught && calls===1 && c.optional(null)===undefined;",
    );
}

#[test]
fn private_values_and_lexical_names_survive_gc_then_release_cycles() {
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
            let script = engine.compile(&realm, "function make(){class C{#payload={value:41};#self=this;#read=()=>{gcNow();return this.#payload.value+1;};reader(){return this.#read;}}let c=new C();return c.reader();}let read=make();gcNow();read()===42;").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
            let update = engine.compile(&realm, "class Counter{#value={valueOf(){gcNow();return 41;}};next(){return ++this.#value;}}new Counter().next()===42;").unwrap();
            assert!(matches!(
                engine.evaluate(&update).unwrap().value(),
                Value::Boolean(true)
            ));
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn private_method_expressions_keep_numeric_coercion_errors_and_class_identity() {
    check(
        "class C{#n=41n;next(){return ++this.#n;}old(){return this.#n--;}}let c=new C();c.next()===42n && c.old()===42n && c.next()===42n;",
    );
    check(
        "let marker={};let calls=0;class C{#n={valueOf(){calls++;throw marker;}};next(){return ++this.#n;}}let c=new C();let caught=false;try{c.next();}catch(e){caught=e===marker;}caught && calls===1;",
    );
    check(
        "function make(v){return class{#n=v;read(){return this.#n;}};}let A=make(41);let B=make(42);let a=new A();let b=new B();let caught=false;try{A.prototype.read.call(b);}catch(e){caught=e instanceof TypeError;}caught && a.read()===41 && b.read()===42;",
    );
    check(
        "class C{#n=41;next(){return ++this.#n;}}let c=new C();let caught=false;try{C.prototype.next.call(new Proxy(c,{}));}catch(e){caught=e instanceof TypeError;}caught && c.next()===42;",
    );
}

#[test]
fn private_parameter_receivers_keep_positions_missing_values_and_defaults() {
    check(
        "let effects=0;class C{#x=42;has(ignored,o){return #x in o;}read(ignored,o){return o.#x;}}let c=new C();let caught=0;try{c.has();}catch(e){if(e instanceof TypeError)caught++;}try{c.read();}catch(e){if(e instanceof TypeError)caught++;}c.has(0,c,effects++) && c.read(0,c)===42 && caught===2 && effects===1;",
    );
    check(
        "let defaults=0;class C{#x=42;has(o=(defaults++,this)){return #x in o;}}let c=new C();c.has() && defaults===1;",
    );
}

#[test]
fn private_arrow_plans_preserve_lexical_this_and_pre_super_tdz() {
    check(
        "class C{#x=41;reader(){return ()=>++this.#x;}}let c=new C();let read=c.reader();read.call({})===42;",
    );
    check(
        "let before=false;let after=false;let brand=false;class A{}class B extends A{#x=42;constructor(){let read=()=>this.#x;let has=o=>#x in o;brand=!has({});try{read();}catch(e){before=e instanceof ReferenceError;}super();after=read()===42;}}new B();before && after && brand;",
    );
}
