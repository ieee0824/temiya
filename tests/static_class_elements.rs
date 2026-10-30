use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        let result = engine
            .evaluate(&script)
            .unwrap_or_else(|error| panic!("{backend:?}: {error}"));
        assert!(
            matches!(result.value(), Value::Boolean(true)),
            "{backend:?}: {:?}",
            result.value()
        );
    }
}

#[test]
fn keys_precede_initializers_and_static_methods_are_already_installed() {
    check(
        "let log='';class C{static [(log+='K','first')]=(log+='A',this.later());static{log+='B';this.second=this.first+1;}static [(log+='L','third')]=(log+='C',this.second+1);static later(){return 40;}}log==='KLABC' && C.first===40 && C.second===41 && C.third===42;",
    );
}

#[test]
fn static_initializers_use_class_binding_this_super_and_undefined_new_target() {
    check(
        "class A{static get seed(){return this.input;}}class B extends A{static input=41;static value=super.seed+1;static target=new.target;static{this.read=()=>super.seed+1;this.self=B;}}B.value===42 && B.read()===42 && B.self===B && B.target===undefined && !A.hasOwnProperty('value');",
    );
}

#[test]
fn fields_define_own_data_properties_and_default_undefined() {
    check(
        "let calls=0;class A{static set x(v){calls++;}}class B extends A{static x=42;static y;}let d=Object.getOwnPropertyDescriptor(B,'x');calls===0 && B.x===42 && d.writable && d.enumerable && d.configurable && B.hasOwnProperty('y') && B.y===undefined;",
    );
}

#[test]
fn static_blocks_have_separate_var_scopes_and_closures() {
    check(
        "let outer=99;class C{static{var outer=41;this.read=()=>outer;}static{var outer=42;this.other=()=>outer;}}outer===99 && C.read()===41 && C.other()===42;",
    );
}

#[test]
fn abrupt_completion_stops_later_initializers_and_preserves_thrown_identity() {
    check(
        "let log='';let marker={};let escaped;let caught=false;try{class C{static first=(escaped=this,log+='A');static{log+='B';throw marker;}static later=(log+='C');}}catch(e){caught=e===marker;}caught && log==='AB' && escaped.hasOwnProperty('first') && !escaped.hasOwnProperty('later');",
    );
}

#[test]
fn static_initialization_can_construct_complete_instance_fields() {
    check("class C{x=42;static instance=new C();}C.instance.x===42 && C.instance instanceof C;");
}

#[test]
fn anonymous_static_initializer_names_and_duplicate_fields_follow_source_order() {
    check(
        "let key=Symbol.iterator;class C{static f=function(){};static arrow=()=>42;static nested=class {};static [key]=function(){};static value=1;static value=42;}C.f.name==='f' && C.arrow.name==='arrow' && C.nested.name==='nested' && C[key].name==='[Symbol.iterator]' && C.value===42;",
    );
}

#[test]
fn key_exceptions_prevent_all_static_initialization() {
    check(
        "let log='';let marker={};let caught=false;try{class C{static first=(log+='A');static [(log+='K',(()=>{throw marker;})())]=1;}}catch(e){caught=e===marker;}caught && log==='K';",
    );
}

#[test]
fn failed_static_definition_evaluates_value_then_stops() {
    check(
        "let effects=0;let escaped;let caught=false;try{class C{static{escaped=this;Object.preventExtensions(this);}static value=(effects++,42);static later=effects++;}}catch(e){caught=e instanceof TypeError;}caught && effects===1 && !escaped.hasOwnProperty('value') && !escaped.hasOwnProperty('later');",
    );
}

#[test]
fn static_closures_and_pending_initializers_survive_gc_then_collect() {
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
            let script=engine.compile(&realm,"function make(){let captured={value:41};let holder={get value(){gcNow();return captured;}};class A{static get seed(){gcNow();return this.saved.value;}}return class extends A{static saved=holder.value;static{gcNow();let increment=1;this.read=()=>super.seed+increment;}static result=(gcNow(),this.read());};}let C=make();let read=C.read;C=undefined;gcNow();read()===42;").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn simple_static_initializers_keep_strict_class_this_and_undefined_new_target() {
    check(
        "function make(){class C{static receiver=this;static target=new.target;}return C;}let C=new make();let caught=false;try{class D{static value=(missingStaticBinding=42);}}catch(e){caught=e instanceof ReferenceError;}C.receiver===C && C.target===undefined && caught && typeof missingStaticBinding==='undefined';",
    );
}
