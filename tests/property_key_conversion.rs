use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

#[test]
fn numeric_keys_keep_canonical_decimal_spelling_and_observable_conversion() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let source = "let object={};object[-0]=1;object[4294967295]=2;object[4294967296]=3;object[1.5]=4;object[-1]=5;object[NaN]=6;object[Infinity]=7;object['01']=8;let calls=0;let key={[Symbol.toPrimitive](hint){if(hint!=='string')throw 42;calls++;return 42;}};object[key]=9;let symbol=Symbol('key');object[symbol]=10;object['0']===1&&object['4294967295']===2&&object['4294967296']===3&&object['1.5']===4&&object['-1']===5&&object['NaN']===6&&object['Infinity']===7&&object['01']===8&&object[42]===9&&object[symbol]===10&&calls===1;";
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
