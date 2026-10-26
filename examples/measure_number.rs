//! Small diagnostic worker: time a numeric Script result through the public API.
//! Run in release mode; the caller controls process isolation and timeouts.

use std::{env, fs, time::Instant};

use temiya::{Engine, benchmark::Measurement, runtime::Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("use cargo run --release --example measure_number".into());
    }
    let arguments: Vec<_> = env::args().skip(1).collect();
    if !(2..=3).contains(&arguments.len()) {
        return Err("usage: measure_number FILE EXPECTED_NUMBER [SAMPLES=15]".into());
    }
    let expected: f64 = arguments[1].parse()?;
    let samples: usize = arguments.get(2).map_or(Ok(15), |text| text.parse())?;
    if !expected.is_finite() || !(1..=1000).contains(&samples) {
        return Err("expected result must be finite and samples must be in 1..=1000".into());
    }
    let source = fs::read_to_string(&arguments[0])?;
    let start = Instant::now();
    let engine = Engine::default();
    let realm = engine.create_realm();
    let realm_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let script = engine.compile(&realm, &source)?;
    let compile_ns = start.elapsed().as_nanos();
    let evaluate = || -> Result<u64, Box<dyn std::error::Error>> {
        let start = Instant::now();
        let result = engine.evaluate(&script)?;
        let elapsed = u64::try_from(start.elapsed().as_nanos())?;
        if !result.value().same_value(&Value::Number(expected)) {
            return Err(format!("expected {expected}, got {:?}", result.value()).into());
        }
        Ok(elapsed)
    };
    let warmup_ns = evaluate()?;
    let values = (0..samples)
        .map(|_| evaluate())
        .collect::<Result<Vec<_>, _>>()?;
    let measurement = Measurement::new("AST evaluate", &arguments[0], values)?;
    let summary = measurement.summary();
    println!(
        "{}",
        serde_json::json!({
            "engine": "Temiya AST (no JIT)", "profile": "release",
            "architecture": env::consts::ARCH, "source": arguments[0],
            "expected": expected, "validated_passes": samples + 1,
            "realm_ns": realm_ns, "compile_ns": compile_ns, "warmup_ns": warmup_ns,
            "samples_ns": measurement.samples_ns, "median_ns": summary.median_ns,
            "mad_ns": summary.mad_ns,
        })
    );
    Ok(())
}
