use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

#[test]
fn data_rest_keeps_key_order_symbols_fresh_attributes_and_fallbacks() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        for source in [
            "let symbol=Symbol('s');let input=Object.create(null);Object.defineProperty(input,'2',{value:20,enumerable:true,writable:false,configurable:false});input.one=22;input[symbol]=7;input['1']=10;Object.defineProperty(input,'hidden',{value:99,enumerable:false});let {'1':drop,...rest}=input;let keys=Reflect.ownKeys(rest);let d=Object.getOwnPropertyDescriptor(rest,'2');drop===10&&keys.length===3&&keys[0]==='2'&&keys[1]==='one'&&keys[2]===symbol&&rest.one===22&&rest[symbol]===7&&d.value===20&&d.writable&&d.enumerable&&d.configurable&&Object.getPrototypeOf(rest)===Object.prototype&&!('hidden' in rest);",
            "let input={keep:20,get later(){delete this.keep;this.extra=7;return 22;}};let {...rest}=input;rest.keep===20&&rest.later===22&&!('extra' in rest)&&!('keep' in input);",
            "let {0:first,...rest}='ab';first==='a'&&rest[1]==='b'&&!('0' in rest)&&!('length' in rest);",
            "let input={};Object.defineProperty(input,'__proto__',{value:42,enumerable:true});let {...rest}=input;Object.getPrototypeOf(rest)===Object.prototype&&Object.getOwnPropertyDescriptor(rest,'__proto__').value===42;",
        ] {
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
}
