//! Requests during public Engine calls that repeatedly create the same closure syntax.
use std::hint::black_box;

use temiya::{Engine, runtime::Value};

#[path = "support/allocations.rs"]
mod allocations;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::default();
    let realm = engine.create_realm();
    engine
        .evaluate(&engine.compile(&realm, "function make(x){return function(y){return x+y;};}")?)?;
    let invocation = engine.compile(&realm, "make(41);")?;
    let operation = || {
        let result = engine.evaluate(black_box(&invocation)).unwrap();
        let Value::Object(function) = result.value() else {
            panic!()
        };
        assert!(
            function
                .call(Value::Undefined, &[Value::Number(1.0)])
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        black_box(result);
    };
    let requests = allocations::count(1000, operation);
    println!(
        "{}",
        serde_json::json!({
            "iterations":1000,"warmup_iterations":128,"validated_calls":1128,
            "profile":if cfg!(debug_assertions){"debug"}else{"release"},
            "scope":"Public Engine evaluates an already compiled invocation, creates a closure and calls it with a checked result. Parsing and Realm/setup creation excluded. No timing instrumentation; requested bytes are not retained bytes or RSS.",
            "allocations":requests,
        })
    );
    Ok(())
}
