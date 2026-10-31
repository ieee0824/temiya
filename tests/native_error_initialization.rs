use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

#[test]
fn caught_native_errors_keep_intrinsic_prototype_descriptors_and_fresh_identity() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let source = "let prototype=TypeError.prototype;let first;let second;TypeError=function(){throw 99;};try{null.value;}catch(e){first=e;}try{null.value;}catch(e){second=e;}let descriptor=Object.getOwnPropertyDescriptor(first,'message');let keys=Reflect.ownKeys(first);let valid=first!==second&&Object.getPrototypeOf(first)===prototype&&keys.length===1&&keys[0]==='message'&&descriptor.writable&&!descriptor.enumerable&&descriptor.configurable&&typeof first.message==='string';first.message='changed';delete first.message;valid&&!('message' in first)||valid&&first.message!== 'changed';";
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}"
        );
    }
}
