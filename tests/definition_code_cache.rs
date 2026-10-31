use temiya::{
    Engine,
    bytecode::{BytecodeCompiler, VirtualMachine},
    embedding::ScriptBackend,
    runtime::{Completion, Realm, Value},
};

#[test]
fn reused_verified_definitions_bind_each_realm_and_do_not_retain_its_heap() {
    let source = "function make(){return {get value(){return answer;}};}make().value;";
    let code = BytecodeCompiler::compile(source).unwrap();
    let independent = BytecodeCompiler::compile(source).unwrap();
    for answer in [41.0, 42.0] {
        let context;
        {
            let realm = Realm::ordinary();
            context = realm.global_object().allocation_context().clone();
            realm
                .global_object()
                .create_data_property("answer".into(), Value::Number(answer))
                .unwrap();
            assert!(
                matches!(VirtualMachine::execute_in_realm(&code, &realm).unwrap(), Completion::Normal(Some(Value::Number(value))) if value == answer)
            );
            assert_eq!(code, independent);
        }
        assert_eq!(context.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn repeated_compiled_factories_keep_private_brands_and_escaping_captures_separate() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        {
            let realm = engine.create_realm();
            let script = engine.compile(&realm, "function make(v){return class{#x=v;read(other){return other.#x;}getter(){return ()=>v;}};}let A=make(41);let B=make(42);let a=new A();let b=new B();let rejected=false;try{b.read(a);}catch(e){rejected=e instanceof TypeError;}let readA=a.getter();let readB=b.getter();rejected&&a.read(a)===41&&b.read(b)===42&&readA()===41&&readB()===42;").unwrap();
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
