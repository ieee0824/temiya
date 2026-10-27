//! Time actual JS scheduling, the public job checkpoint and completion lookup.
use std::{env, fs, time::Instant};

use temiya::{Engine, runtime::Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("build this worker in release mode".into());
    }
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: measure_async_workload WORKLOAD_JSON SAMPLES".into());
    }
    let samples: usize = args[1].parse()?;
    if !(1..=1000).contains(&samples) {
        return Err("samples must be in 1..=1000".into());
    }
    let input: serde_json::Value = serde_json::from_str(&fs::read_to_string(&args[0])?)?;
    let source = |key: &str| input[key].as_str().ok_or("missing source");
    let setup = source("setup")?;
    let invocation = source("invocation")?;
    let completion = source("completion")?;
    let expected = input["expected"]
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or("invalid expected")?;
    let before = input["before_drain_expected"]
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or("invalid pre-checkpoint expectation")?;
    let start = Instant::now();
    let engine = Engine::default();
    let realm = engine.create_realm();
    let startup_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let script = engine.compile(&realm, setup)?;
    let setup_backend = script.backend();
    engine.evaluate(&script)?;
    drop(script);
    let setup_ns = start.elapsed().as_nanos();
    let run = || -> Result<_, Box<dyn std::error::Error>> {
        let start = Instant::now();
        let script = engine.compile(&realm, invocation)?;
        let invocation_backend = script.backend();
        let scheduled = engine.evaluate(&script)?;
        drop(script);
        if !scheduled.value().same_value(&Value::Number(before)) {
            return Err("unexpected result before checkpoint; callbacks may have run early".into());
        }
        let drained_jobs = engine.drain_jobs(&realm)?;
        let script = engine.compile(&realm, completion)?;
        let completion_backend = script.backend();
        let value = engine.evaluate(&script)?;
        drop(script);
        let elapsed = u64::try_from(start.elapsed().as_nanos())?;
        if drained_jobs == 0 || !value.value().same_value(&Value::Number(expected)) {
            return Err(format!(
                "expected {expected} after actual queued jobs, got {:?} ({drained_jobs} jobs)",
                value.value()
            )
            .into());
        }
        Ok((
            elapsed,
            drained_jobs,
            invocation_backend,
            completion_backend,
        ))
    };
    let warmup = run()?;
    let passes = (0..samples).map(|_| run()).collect::<Result<Vec<_>, _>>()?;
    println!(
        "{}",
        serde_json::json!({
            "engine":"Temiya", "mode":"Auto Script", "profile":"release",
            "architecture":env::consts::ARCH,
            "scope":"parse+evaluate scheduling; drain jobs; parse+evaluate completion; setup excluded",
            "expected":expected, "before_drain_expected":before, "validated_passes":samples+1,
            "startup_ns":startup_ns, "setup_ns":setup_ns, "setup_backend":format!("{setup_backend:?}"),
            "warmup_ns":warmup.0, "warmup_drained_jobs":warmup.1,
            "samples_ns":passes.iter().map(|p| p.0).collect::<Vec<_>>(),
            "drained_jobs":passes.iter().map(|p| p.1).collect::<Vec<_>>(),
            "invocation_backends":passes.iter().map(|p| format!("{:?}",p.2)).collect::<Vec<_>>(),
            "completion_backends":passes.iter().map(|p| format!("{:?}",p.3)).collect::<Vec<_>>(),
            "sandbox":null
        })
    );
    Ok(())
}
