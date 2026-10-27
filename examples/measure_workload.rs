//! Match the public omoikane worker's parse-plus-invocation timing boundary.
use std::{env, fs, time::Instant};

use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("build this worker in release mode".into());
    }
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: measure_workload WORKLOAD_JSON SAMPLES".into());
    }
    let samples: usize = args[1].parse()?;
    if !(1..=1000).contains(&samples) {
        return Err("samples must be in 1..=1000".into());
    }
    let workload: serde_json::Value = serde_json::from_str(&fs::read_to_string(&args[0])?)?;
    let setup = workload["setup"].as_str().ok_or("missing setup")?;
    let invocation = workload["invocation"]
        .as_str()
        .ok_or("missing invocation")?;
    let expected = workload["expected"]
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or("invalid expected result")?;
    let start = Instant::now();
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Auto)
        .build();
    let realm = engine.create_realm();
    let startup_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    engine.evaluate(&engine.compile(&realm, setup)?)?;
    let setup_ns = start.elapsed().as_nanos();
    let evaluate = || -> Result<u64, Box<dyn std::error::Error>> {
        let start = Instant::now();
        let value = engine.evaluate(&engine.compile(&realm, invocation)?)?;
        let elapsed = u64::try_from(start.elapsed().as_nanos())?;
        if !value.value().same_value(&Value::Number(expected)) {
            return Err(format!("expected {expected}, got {:?}", value.value()).into());
        }
        Ok(elapsed)
    };
    let warmup_ns = evaluate()?;
    let samples_ns = (0..samples)
        .map(|_| evaluate())
        .collect::<Result<Vec<_>, _>>()?;
    println!(
        "{}",
        serde_json::json!({
            "engine": "Temiya", "mode": "Auto Script+loop",
            "scope": "parse+evaluate invocation; function setup excluded",
            "profile": "release", "architecture": env::consts::ARCH,
            "expected": expected, "validated_passes": samples + 1,
            "startup_ns": startup_ns, "setup_ns": setup_ns,
            "warmup_ns": warmup_ns, "samples_ns": samples_ns,
            "sandbox": null,
        })
    );
    Ok(())
}
