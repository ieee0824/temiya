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
fn declarations_parameters_defaults_elisions_nested_and_rest() {
    check(
        "let [a,[b=22],...rest]=[20,[],1,2];function sum([x,y=x],z=y){return x+y+z;}let skipped=0;let input={[Symbol.iterator](){let n=0;return {next(){n++;return {done:n>2,get value(){if(n===1)throw 42;return 7;}};}};}};let [,v]=input; a+b===42&&rest.length===2&&Array.isArray(rest)&&rest[1]===2&&sum([14])===42&&v===7;",
    );
    check("var [a,b=22]=[20];a+b===42;");
}

#[test]
fn tdz_and_abrupt_close_preserve_original_throw() {
    check(
        "let closes=0;let input={[Symbol.iterator](){return {next(){return {done:false,value:undefined};},return(){closes++;throw 99;}};}};let caught=false;try{let [a=a]=input;}catch(e){caught=e instanceof ReferenceError;}caught&&closes===1;",
    );
    check(
        "let caught=false;function f([a=b],b=42){return a;}try{f([]);}catch(e){caught=e instanceof ReferenceError;}caught;",
    );
}

#[test]
fn exhausted_iterator_is_not_advanced_and_empty_pattern_closes() {
    check(
        "let calls=0;let closes=0;let input={[Symbol.iterator](){return {next(){calls++;return {done:true,get value(){throw 42;}};},return(){closes++;return {};}};}};let [a=20,,b=22,...rest]=input;let []=input;a+b===42&&calls===1&&closes===1&&rest.length===0;",
    );
}

#[test]
fn catch_and_loop_declarations_initialize_patterns_in_their_scope() {
    check(
        "let answer=0;try{throw [20,[22]];}catch([a,[b]]){answer=a+b;}let total=0;for(let [a,b] of [[1,2],[3,4]]){total+=a+b;}let v=0;for(var [x,y] of [[20,22]]){v=x+y;}answer===42&&total===10&&v===42&&x===20;",
    );
    check("let total=0;for(let [i]=[0];i<3;i++){total+=i;}total===3;");
    check(
        "let outer=7;let plain=0;let pattern=0;let noParameter=0;let escaped;try{throw 42;}catch(outer){plain=outer;outer=20;escaped=()=>outer;}try{throw [];}catch([first=outer,second=first+1]){pattern=first+second;}try{throw 99;}catch{let outer=22;noParameter=outer;}outer===7&&plain===42&&escaped()===20&&pattern===15&&noParameter===22&&typeof first==='undefined'&&typeof second==='undefined';",
    );
}

#[test]
fn defaults_keep_names_and_step_errors_do_not_close() {
    check("let [f=function(){},C=class{}]=[];f.name==='f'&&C.name==='C';");
    for step in [
        "next(){throw marker;}",
        "next(){return {get done(){throw marker;}};}",
        "next(){return {done:false,get value(){throw marker;}};}",
    ] {
        check(&format!(
            "let marker={{}};let closes=0;let input={{[Symbol.iterator](){{return {{{step},return(){{closes++;return {{}};}}}};}}}};let caught=false;try{{let [value]=input;}}catch(e){{caught=e===marker;}}caught&&closes===0;"
        ));
    }
}

#[test]
fn binding_values_and_environments_survive_reentrant_gc_then_release() {
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
            let source = "function make(){let index=0;return {[Symbol.iterator](){gcNow();return {next(){gcNow();let current=++index;return {get done(){gcNow();return current>3;},get value(){gcNow();return current===1?undefined:{answer:current===2?20:22};}};},return(){gcNow();return {};}};}};}function bind([first=(gcNow(),{answer:1}),...rest]){gcNow();return first.answer+rest[0].answer+rest[1].answer;}let [a=(gcNow(),{answer:20}),b]=[undefined,{answer:22}];gcNow();bind(make())===43&&a.answer+b.answer===42;";
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
fn parameter_bound_arguments_shadows_the_implicit_arguments_object_and_has_tdz() {
    check(
        "function array([arguments]){return arguments;}function defaults(arguments=22){return arguments;}function rest(...arguments){return Array.isArray(arguments)&&arguments[0]===20&&arguments.length===2;}let caught=false;function later(value=arguments,arguments=42){return value;}try{later();}catch(e){caught=e instanceof ReferenceError;}function Constructor([arguments]){this.answer=arguments;}array([20])+defaults()===42&&rest(20,22)&&caught&&new Constructor([42]).answer===42;",
    );
}

#[test]
fn nested_abrupt_binding_closes_inner_then_outer_and_keeps_original_throw() {
    check(
        "let marker={};let trace='';function fail(){throw marker;}let inner={[Symbol.iterator](){return {next(){return {done:false,value:undefined};},get return(){trace+='i';return function(){trace+='I';throw 20;};}};}};let outer={[Symbol.iterator](){return {next(){return {done:false,value:inner};},get return(){trace+='o';return function(){trace+='O';throw 22;};}};}};let caught=false;try{let [[value=fail()]]=outer;}catch(e){caught=e===marker;}caught&&trace==='iIoO';",
    );
}

#[test]
fn var_pattern_resolves_object_environment_before_stepping() {
    check(
        "let trace='';let object=new Proxy({x:0},{has(target,key){if(key==='x'){trace+='H';return true;}return false;},set(target,key,value,receiver){trace+='S';return Reflect.set(target,key,value,receiver);}});let input={[Symbol.iterator](){return {next(){trace+='N';return {done:false,get value(){trace+='V';return 42;}};},return(){trace+='R';return {};}};}};with(object){var [x]=input;}object.x===42&&trace[0]==='H'&&trace[1]==='N'&&trace[2]==='V'&&trace[trace.length-2]==='S'&&trace[trace.length-1]==='R';",
    );
}

#[test]
fn rest_uses_own_data_properties_and_nested_patterns_accept_strings() {
    check(
        "let writes=0;Object.defineProperty(Array.prototype,'0',{set(value){writes++;},configurable:true});let [first,...rest]=[20,22];let descriptor=Object.getOwnPropertyDescriptor(rest,'0');delete Array.prototype[0];let [character,missing='default',...empty]='😀';let [head,...[tail]]=[20,22];first+rest[0]===42&&writes===0&&Array.isArray(rest)&&descriptor.value===22&&descriptor.writable&&descriptor.enumerable&&descriptor.configurable&&character==='😀'&&missing==='default'&&empty.length===0&&head+tail===42;",
    );
}

#[test]
fn prepared_parameter_names_keep_fresh_tdz_cells_across_reentrant_calls() {
    check(
        "let saved=[];function f([a=()=>b],b=42){saved.push(a);return b;}f([],20);f([],22);let calls=0;function g([x=(calls++===0?g([7]):9)]){return x;}saved[0]()===20&&saved[1]()===22&&g([])===7&&g([])===9;",
    );
    check(
        "let calls=0;function f([a=b],b=42){return a;}for(let i=0;i<2;i++){try{f([]);}catch(e){if(e instanceof ReferenceError)calls++;}}calls===2;",
    );
}
