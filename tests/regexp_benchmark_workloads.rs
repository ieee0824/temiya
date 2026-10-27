use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

#[test]
fn authored_regexp_inputs_work_through_public_engine_on_repeated_passes() {
    let inputs = [
        (
            include_str!("../benchmarks/regexp-inputs/regexp-construction.json"),
            128 * (8 + 5 + 2),
        ),
        (
            include_str!("../benchmarks/regexp-inputs/regexp-global-exec.json"),
            2048 * (3 + 4 + 2),
        ),
        (
            include_str!("../benchmarks/regexp-inputs/regexp-unicode-captures.json"),
            1024 * (4 + 2),
        ),
    ];
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        for (json, expected) in inputs {
            let input: serde_json::Value = serde_json::from_str(json).unwrap();
            assert_eq!(input["expected"].as_i64(), Some(expected));
            let engine = Engine::builder().script_backend(backend).build();
            let realm = engine.create_realm();
            let setup = engine
                .compile(&realm, input["setup"].as_str().unwrap())
                .unwrap();
            if backend == ScriptBackend::Auto {
                assert_eq!(setup.backend(), ScriptBackend::Bytecode);
            }
            engine.evaluate(&setup).unwrap();
            for _ in 0..2 {
                let invocation = engine
                    .compile(&realm, input["invocation"].as_str().unwrap())
                    .unwrap();
                if backend == ScriptBackend::Auto {
                    assert_eq!(invocation.backend(), ScriptBackend::Bytecode);
                }
                let result = engine.evaluate(&invocation).unwrap();
                assert!(result.value().same_value(&Value::Number(expected as f64)));
            }
        }
    }
}
