use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        let value = engine
            .evaluate(&script)
            .unwrap_or_else(|error| panic!("{backend:?}: {error}: {source}"));
        assert!(
            matches!(value.value(), Value::Boolean(true)),
            "{backend:?}: {:?}: {source}",
            value.value()
        );
    }
}

#[test]
fn object_and_class_super_calls_preserve_the_actual_receiver() {
    check(
        "let base={read(){return this.value;}};let object={__proto__:base,value:42,read(){return super.read();}};let other={value:99,read:object.read};object.read()===42 && other.read()===99;",
    );
    check(
        "class A{read(){return this.value;}static read(){return this.value;}}class B extends A{constructor(){super();this.value=42;}read(){return super.read();}static read(){return super.read();}}B.value=99;new B().read()===42 && B.read()===99;",
    );
}

#[test]
fn inherited_accessors_and_data_writes_use_the_receiver() {
    check(
        "let base={get value(){return this.stored;},set value(v){this.stored=v;}};let object={__proto__:base,stored:20,step(){super.value+=22;return super.value;}};object.step()===42 && object.stored===42 && base.stored===undefined;",
    );
    check(
        "let base={value:20};let object={__proto__:base,write(){super.value=42;return this.value;}};object.write()===42 && base.value===20 && Object.getOwnPropertyDescriptor(object,'value').value===42;",
    );
    check(
        "let base={};Object.defineProperty(base,'value',{value:1,writable:false});class A{}Object.setPrototypeOf(A.prototype,base);class B extends A{write(){super.value=42;}}let caught=false;try{new B().write();}catch(e){caught=e instanceof TypeError;}caught && base.value===1;",
    );
}

#[test]
fn home_object_stays_with_the_method_and_its_parent_is_read_on_each_access() {
    check(
        "let first={read(){return this.value;}},second={read(){return this.value*2;}};let object={__proto__:first,value:21,read(){return super.read();}};let other={__proto__:second,value:42,read:object.read};let before=other.read();Object.setPrototypeOf(object,second);before===42 && object.read()===42 && other.read()===84;",
    );
    check(
        "class A{static read(){return this.value;}}class B extends A{static read(){return super.read();}}B.value=21;let first=B.read();Object.setPrototypeOf(B,{read(){return this.value*2;}});first===21 && B.read()===42;",
    );
}

#[test]
fn arrows_retain_super_and_their_lexical_receiver() {
    check(
        "let base={read(){return this.value;}};let object={__proto__:base,value:42,make(){return ()=>()=>super.read();}};let read=object.make()();let other={value:99,read};read()===42 && other.read()===42;",
    );
    check(
        "class A{read(){return this.value;}}class B extends A{constructor(){let read=()=>super.read();let early=false;try{read();}catch(e){early=e instanceof ReferenceError;}super();this.value=42;this.read=read;this.early=early;}}let object=new B();object.early && object.read()===42;",
    );
}

#[test]
fn computed_key_conversion_is_deferred_and_cached_for_updates() {
    check(
        "let calls=0;let key={toString(){calls++;return 'value';}};let base={get value(){return this.stored;},set value(v){this.stored=v;}};let object={__proto__:base,stored:41,step(){return ++super[key];}};object.step()===42 && object.stored===42 && calls===1 && base.stored===undefined;",
    );
    check(
        "let log='';let key={toString(){log+='K';return 'value';}};let base={set value(v){log+='S';this.stored=v;}};let other={set value(v){throw 99;}};let object={__proto__:base,write(){return super[key]=(log+='R',Object.setPrototypeOf(object,other),42);}};object.write()===42 && object.stored===42 && log==='RKS';",
    );
    check(
        "let first={value:1},second={value:42};let object={__proto__:first,read(){return super[(Object.setPrototypeOf(object,second),'value')];}};object.read()===42;",
    );
}

#[test]
fn uninitialized_this_and_null_bases_preserve_error_order() {
    check(
        "let effects=0;class A{}class B extends A{constructor(){super[(effects++,'value')];}}let caught=false;try{new B();}catch(e){caught=e instanceof ReferenceError;}caught && effects===0;",
    );
    check(
        "let effects=0;let key={toString(){effects++;return 'value';}};let object={__proto__:null,read(){return super[key];}};let caught=false;try{object.read();}catch(e){caught=e instanceof TypeError;}caught && effects===0;",
    );
}

#[test]
fn delete_super_references_throws_before_key_conversion_even_with_null_base() {
    check(
        "let effects=0;let key={toString(){effects++;return 'value';}};let object={__proto__:null,remove(){delete super[(effects++,key)];}};let caught=false;try{object.remove();}catch(e){caught=e instanceof ReferenceError;}caught && effects===1;",
    );
}

#[test]
fn proxy_get_and_set_traps_receive_the_original_this() {
    check(
        "let receiver;let base=new Proxy({},{get(target,key,actual){receiver=actual;return 42;},set(target,key,value,actual){receiver=actual;Object.defineProperty(actual,'stored',{value,writable:true,configurable:true});return true;}});let object={__proto__:base,read(){return super.value;},write(){super.value=42;}};let value=object.read();let readReceiver=receiver;object.write();value===42 && readReceiver===object && receiver===object && object.stored===42;",
    );
}

#[test]
fn detached_strict_methods_preserve_primitive_and_undefined_this() {
    check(
        "class A{read(){return this;}}class B extends A{read(){return super.read();}}let read=B.prototype.read;read()===undefined && read.call(42)===42;",
    );
}

#[test]
fn captured_home_objects_and_receivers_survive_reentrant_collection() {
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
            let script = engine.compile(&realm, "function make(receiver){return {__proto__:{read(){gcNow();return this.value;}},make(){return ()=>super.read();}}.make.call(receiver);}let read=make({value:42});gcNow();read()===42;").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
            engine.collect_garbage().unwrap();
            let script = engine.compile(&realm, "read()===42;").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn super_assignment_checks_receiver_descriptors_and_propagates_proxy_errors() {
    check(
        "let calls=0;let actual={set value(v){calls++;}};class A{}class B extends A{write(){super.value=42;}}let caught=false;try{B.prototype.write.call(actual);}catch(e){caught=e instanceof TypeError;}caught && calls===0 && Object.getOwnPropertyDescriptor(actual,'value').set!==undefined;",
    );
    check(
        "let base={value:1};let home={__proto__:base,write(){super.value=42;}};let marker={};let receiver=new Proxy({},{getOwnPropertyDescriptor(){throw marker;}});let caught=false;try{home.write.call(receiver);}catch(e){caught=e===marker;}caught && base.value===1;",
    );
}
