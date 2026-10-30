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
fn base_fields_precede_parameters_and_constructor_body() {
    check(
        "let log='';class C{x=(log+='F',42);constructor(value=(log+='P',this.x)){log+='B';this.seen=value;}}let a=new C();let b=new C(99);log==='FPBFB' && a.x===42 && a.seen===42 && b.seen===99;",
    );
    check(
        "class C{x;y=42;}let a=new C();let d=Object.getOwnPropertyDescriptor(a,'x');a.hasOwnProperty('x') && a.x===undefined && a.y===42 && d.writable && d.enumerable && d.configurable;",
    );
}

#[test]
fn derived_explicit_default_and_delegating_paths_initialize_own_fields() {
    check(
        "let log='';class A{x=(log+='A',1);constructor(){log+='B';this.seed=41;}}class B extends A{x=(log+='D',this.seed+1);constructor(){log+='C';super();log+=this.x===42?'E':'!';}}let b=new B();log==='CABDE' && b.x===42;",
    );
    check(
        "class A{constructor(n){this.seed=n;}}class B extends A{x=this.seed+1;}class C extends A{x=this.seed+2;constructor(n){super(n);}}new B(41).x===42 && new C(40).x===42;",
    );
    check(
        "let count=0;class A{}class B extends A{x=++count;constructor(){return {};}}let b=new B();count===0 && !b.hasOwnProperty('x');",
    );
}

#[test]
fn computed_keys_run_once_and_initializers_run_per_instance_in_order() {
    check(
        "let log='';let key={toString(){log+='K';return 'x';}};class C{[(log+='E',key)]=(log+='X',1);y=(log+='Y',this.x+1);[(log+='L','x')]=(log+='Z',3);}let before=log;let a=new C();let b=new C();before==='EKL' && log==='EKLXYZXYZ' && a.x===3 && a.y===2 && b.x===3;",
    );
    check(
        "let key=Symbol.iterator;let other=Symbol.toStringTag;class C{[key]=42;[other];['__proto__']=99;}let c=new C();typeof key==='symbol' && typeof other==='symbol' && key!==other && c[key]===42 && c[other]===undefined && c.hasOwnProperty(other) && c.hasOwnProperty('__proto__') && c.__proto__===99 && Object.getPrototypeOf(c)===C.prototype;",
    );
}

#[test]
fn fields_define_own_data_and_propagate_definition_errors() {
    check(
        "let calls=0;class A{set x(value){calls++;}}class B extends A{x=42;}let b=new B();calls===0 && b.x===42 && Object.getOwnPropertyDescriptor(b,'x').value===42;",
    );
    check(
        "let receiver={};Object.preventExtensions(receiver);let effects=0;class A{constructor(){return receiver;}}class B extends A{x=(effects++,42);y=(effects++,99);}let caught=false;try{new B();}catch(e){caught=e instanceof TypeError;}caught && effects===1;",
    );
    check(
        "let marker={};let effects=0;class C{x=(effects++,(()=>{throw marker;})());y=(effects++,42);}let caught=false;try{new C();}catch(e){caught=e===marker;}caught && effects===1;",
    );
}

#[test]
fn field_arrows_capture_distinct_this_and_super_home() {
    check(
        "class A{get value(){return this.seed;}}class C extends A{seed=42;read=()=>super.value;target=new.target;}let a=new C();let b=new C();b.seed=99;let read=a.read;read()===42 && b.read()===99 && a.target===undefined;",
    );
    check(
        "let first={value:1};let second={value:42};class C{x=super.value;}Object.setPrototypeOf(C.prototype,first);let a=new C();Object.setPrototypeOf(C.prototype,second);let b=new C();a.x===1 && b.x===42;",
    );
}

#[test]
fn anonymous_initializer_names_do_not_introduce_name_bindings() {
    check(
        "class C{f=function(){return typeof f;};a=()=>42;c=class {};}let c=new C();c.f.name==='f' && c.a.name==='a' && c.c.name==='c' && c.f()==='undefined';",
    );
}

#[test]
fn proxy_receiver_and_repeated_super_preserve_definition_order() {
    check(
        "let log='';let target={};let receiver=new Proxy(target,{defineProperty(object,key,descriptor){log+=key;Object.defineProperty(object,key,descriptor);return true;}});class A{constructor(){return receiver;}}class B extends A{x=41;y=this.x+1;}let b=new B();b===receiver && log==='xy' && target.y===42;",
    );
    check(
        "let count=0;class A{}class B extends A{x=++count;constructor(){super();let caught=false;try{super();}catch(e){caught=e instanceof ReferenceError;}this.caught=caught;}}let b=new B();b.caught && count===1 && b.x===1;",
    );
}

#[test]
fn initializer_closures_and_home_objects_survive_reentrant_gc_and_collect() {
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
            let script=engine.compile(&realm,"function make(){let captured={value:41};class A{get value(){gcNow();return this.seed.value+1;}}return class extends A{seed=(gcNow(),captured);read=()=>super.value;};}let C=make();gcNow();let object=new C();let read=object.read;C=undefined;object=undefined;gcNow();read()===42;").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn symbol_initializer_names_use_the_actual_key_description() {
    check(
        "let key=Symbol.iterator;let other=Symbol.toStringTag;class C{[key]=function(){};[other]=class {};}let c=new C();typeof key==='symbol' && key!==other && c[key].name==='[Symbol.iterator]' && c[other].name==='[Symbol.toStringTag]';",
    );
}
