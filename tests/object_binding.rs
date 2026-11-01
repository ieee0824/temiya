use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}: {source}"
        );
    }
}

#[test]
fn declarations_parameters_nested_arrays_catch_and_loops() {
    check(
        "let {a=20,nested:{b},list:[c]}={nested:{b:22},list:[42]};function read({x,y=x}){return x+y;}let caught=0;try{throw {answer:42};}catch({answer}){caught=answer;}let sum=0;for(let {value} of [{value:20},{value:22}])sum+=value;var {global=42}={};a+b===42&&c===42&&read({x:21})===42&&caught===42&&sum===42&&global===42;",
    );
}

#[test]
fn computed_key_conversion_default_and_get_order() {
    check(
        "let order='';let key={[Symbol.toPrimitive](hint){if(hint!=='string')throw 42;order+='k';return 'answer';}};let {[key]:value=(order+='d',42)}={get answer(){order+='g';return undefined;}};value===42&&order==='kgd';",
    );
    check(
        "let {'a':a,0x10:number,__proto__:proto}={a:20,16:22,__proto__:null};a+number===42&&proto===undefined;",
    );
}

#[test]
fn rest_excludes_strings_and_symbols_and_creates_data_properties() {
    check(
        "let keep=Symbol('keep'),drop=Symbol('drop');let input={first:20,second:22,[keep]:42,[drop]:99};Object.defineProperty(input,'hidden',{value:99,enumerable:false});if(input[keep]!==42)throw 13;let {first,[drop]:removed,...rest}=input;let keys=Reflect.ownKeys(rest),descriptor=Object.getOwnPropertyDescriptor(rest,'second');if(first+rest.second!==42)throw 1;if(removed!==99)throw 2;if(rest[keep]!==42)throw 3;if(keys.length!==2)throw 4;if(keys[0]!=='second')throw 5;if(keys[1]!==keep)throw 6;if('first' in rest)throw 7;if('hidden' in rest)throw 8;if(drop in rest)throw 9;if(!descriptor.writable)throw 10;if(!descriptor.enumerable)throw 11;if(!descriptor.configurable)throw 12;true;",
    );
    check(
        "let writes=0;Object.defineProperty(Object.prototype,'answer',{set(value){writes++;},configurable:true});let {...rest}={answer:42};let descriptor=Object.getOwnPropertyDescriptor(rest,'answer');delete Object.prototype.answer;writes===0&&descriptor.value===42&&Object.getPrototypeOf(rest)===Object.prototype;",
    );
}

#[test]
fn rest_proxy_exclusions_precede_descriptor_and_get_traps() {
    check(
        "let trace='';let input=new Proxy({take:20,keep:22},{ownKeys(target){trace+='K';return ['take','keep'];},getOwnPropertyDescriptor(target,key){if(key==='take')throw 99;trace+='D';return Reflect.getOwnPropertyDescriptor(target,key);},get(target,key,receiver){trace+=key==='take'?'T':'G';return Reflect.get(target,key,receiver);}});let {take,...rest}=input;take+rest.keep===42&&trace==='TKDG';",
    );
}

#[test]
fn abrupt_completions_tdz_and_nullish_sources() {
    check(
        "let marker={},caught=false;try{let {value}={get value(){throw marker;}};}catch(e){caught=e===marker;}let tdz=false;try{let {value=value}={};}catch(e){tdz=e instanceof ReferenceError;}let ran=0;let nil=false;try{let {[++ran]:value}=null;}catch(e){nil=e instanceof TypeError;}let empty=false;try{let {}=undefined;}catch(e){empty=e instanceof TypeError;}caught&&tdz&&nil&&empty&&ran===0;",
    );
    check(
        "let closes=0,marker={},caught=false;let input={[Symbol.iterator](){return {next(){return {done:false,value:{get value(){throw marker;}}};},return(){closes++;throw 99;}};}};try{let [{value}]=input;}catch(e){caught=e===marker;}caught&&closes===1;",
    );
    check(
        "function read({arguments}){return arguments;}let caught=false;function later({value=arguments},arguments=42){return value;}try{later({});}catch(e){caught=e instanceof ReferenceError;}read({arguments:42})===42&&caught;",
    );
}

#[test]
fn primitives_keep_original_receiver_and_rest_copies_string_code_units() {
    check(
        "Object.defineProperty(Number.prototype,'answer',{get:function(){'use strict';return this===42?20:0;},configurable:true});let {answer}=42;let {0:first,...rest}='😀x';let units=first.length===1&&rest[1].length===1&&rest[2]==='x';delete Number.prototype.answer;answer===20&&units&&!('0' in rest)&&Reflect.ownKeys(rest).length===2;",
    );
}

#[test]
fn var_reference_is_resolved_before_getter_and_default() {
    check(
        "let trace='';let scope=new Proxy({value:0},{has(target,key){if(key==='value'){trace+='H';return true;}return false;},set(target,key,value,receiver){trace+='S';return Reflect.set(target,key,value,receiver);}});let input={get answer(){trace+='G';return undefined;}};with(scope){var {answer:value=(trace+='D',42)}=input;}scope.value===42&&trace==='HGDS';",
    );
}

#[test]
fn symbol_key_factory_is_unique_coerces_descriptions_and_cannot_construct() {
    check(
        "let order='';let description={[Symbol.toPrimitive](hint){order+=hint;return 'key';}};let first=Symbol(description),second=Symbol('key');let caught=false;try{new Symbol();}catch(e){caught=e instanceof TypeError;}let bad=false;try{Symbol(first);}catch(e){bad=e instanceof TypeError;}let input={[first]:20,[second]:22};let {[first]:value,...rest}=input;typeof first==='symbol'&&first!==second&&value+rest[second]===42&&order==='string'&&caught&&bad;",
    );
}

#[test]
fn symbol_is_a_constructor_but_its_construct_operation_throws() {
    check(
        "let receiver=Reflect.construct(function(){this.answer=42;},[],Symbol);class Derived extends Symbol{}let threw=false;try{new Derived();}catch(e){threw=e instanceof TypeError;}let direct=false;try{new Symbol();}catch(e){direct=e instanceof TypeError;}receiver.answer===42&&Object.getPrototypeOf(receiver)===Symbol.prototype&&Object.getPrototypeOf(Symbol)===Function.prototype&&threw&&direct;",
    );
}

#[test]
fn reentrant_collection_preserves_source_and_rest_and_releases_everything() {
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
            let source = "let key={[Symbol.toPrimitive](){gcNow();return 'first';}};function read({[key]:first=(gcNow(),{answer:20}),...rest}){gcNow();return first.answer+rest.last.answer;}read({get first(){gcNow();return undefined;},get last(){gcNow();return {answer:22};}})===42;";
            let script = engine.compile(&realm, source).unwrap();
            assert!(
                matches!(
                    engine.evaluate(&script).unwrap().value(),
                    Value::Boolean(true)
                ),
                "{backend:?}"
            );
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn prepared_object_parameters_keep_computed_keys_and_fresh_tdz_cells() {
    check(
        "let saved=[],calls=0;let key={[Symbol.toPrimitive](){calls++;return 'a';}};function f({[key]:a=()=>b,...rest},b=42){saved.push(a);return rest.k;}let x=f({k:1},20),y=f({k:2},22);let ran=0;function g({a=(ran++===0?g({a:7}):9)}){return a;}x===1&&y===2&&calls===2&&saved[0]()===20&&saved[1]()===22&&g({})===7&&g({})===9;",
    );
    check(
        "let calls=0;function f({a=b},b=42){return a;}for(let i=0;i<2;i++){try{f({});}catch(e){if(e instanceof ReferenceError)calls++;}}calls===2;",
    );
}

#[test]
fn catch_identifier_cells_survive_closures_and_mutation() {
    check(
        "let readers=[];for(let i=0;i<3;i++){try{throw i;}catch(error){readers.push(()=>error);error+=10;}}readers[0]()===10&&readers[1]()===11&&readers[2]()===12;",
    );
}

#[test]
fn rest_reads_changed_data_and_keeps_proxy_get_distinct_from_descriptor() {
    check(
        "let inherited=0;let input={get first(){input.later=42;delete input.gone;return 20;},later:1,gone:99};Object.setPrototypeOf(input,{get gone(){inherited++;return 99;}});let {...rest}=input;rest.first===20&&rest.later===42&&!Object.prototype.hasOwnProperty.call(rest,'gone')&&inherited===0;",
    );
    check(
        "let calls=0;let source=new Proxy({value:1},{get(target,key){calls++;return 42;}});let {...rest}=source;rest.value===42&&calls===1;",
    );
}

#[test]
fn rest_accessor_copy_preserves_missing_getters_and_proxy_callable_receivers() {
    check(
        "let calls=0;let source={set empty(value){calls++;}};Object.defineProperty(source,'hidden',{get(){throw 42;},enumerable:false});let {...rest}=source;Object.prototype.hasOwnProperty.call(rest,'empty')&&rest.empty===undefined&&!Object.prototype.hasOwnProperty.call(rest,'hidden')&&calls===0;",
    );
    check(
        "let source={marker:42};let seen=false;let getter=new Proxy(function(){return this.marker;},{apply(target,receiver,args){seen=receiver===source&&args.length===0;return Reflect.apply(target,receiver,args);}});Object.defineProperty(source,'answer',{get:getter,enumerable:true});let {...rest}=source;rest.answer===42&&seen;",
    );
}

#[test]
fn repeated_nullish_binding_errors_keep_fresh_exception_identity_and_messages() {
    check(
        "let first,second;try{let {}=null;}catch(error){first=error;error.message='changed';}try{let {}=undefined;}catch(error){second=error;}first!==second&&first instanceof TypeError&&second instanceof TypeError&&first.message==='changed'&&second.message==='cannot destructure null or undefined';",
    );
}

#[test]
fn nullish_bytecode_patterns_fail_before_keys_and_uninitialized_this() {
    check(
        "let keys=0,defaults=0;class Base{}class Derived extends Base{constructor(){let first,second;try{let {[++keys]:x=(++defaults)}=null;}catch(e){first=e;}try{let {...rest}=undefined;}catch(e){second=e;}super();this.ok=first instanceof TypeError&&second instanceof TypeError&&first!==second;}}new Derived().ok&&keys===0&&defaults===0;",
    );
    check(
        "let keys=0;function run(){let read;try{let {[++keys]:x}=null;}catch({message}){read=message;}return typeof read==='string'&&keys===0;}run();",
    );
}

#[test]
fn static_bytecode_pattern_preserves_getters_proxy_receiver_and_tdz_order() {
    check(
        "function run(){let trace='',read;let {a,b}={get a(){trace+='a';read=()=>a;return 20;},get b(){trace+='b';return read()+2;}};return a+b===42&&trace==='ab';}run();",
    );
    check(
        "function run(){let source,seen;source=new Proxy({x:42},{get(target,key,receiver){seen=receiver;return target[key];}});let {x}=source;return x===42&&seen===source;}run();",
    );
    check(
        "function run(){let caught=false;try{let {a,b}={get a(){return b;},b:42};}catch(e){caught=e instanceof ReferenceError;}return caught;}run();",
    );
}
