use temiya::{Engine, ScriptBackend, runtime::Value};

#[test]
fn number_pairs_match_independent_javascript_oracle_in_ast_and_vm() {
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("number-binary-oracle.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1053);
    for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        for case in cases {
            let source = case["expression"].as_str().unwrap();
            let expected = case["expected"].as_str().unwrap();
            let expected = match expected {
                "true" => Value::Boolean(true),
                "false" => Value::Boolean(false),
                number => Value::Number(number.parse().unwrap()),
            };
            let script = engine.compile(&realm, source).unwrap();
            let actual = engine.evaluate(&script).unwrap();
            assert!(
                actual.value().same_value(&expected),
                "{backend:?}: {source}: {actual:?}, expected {expected:?}"
            );
        }
    }
}
