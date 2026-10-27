//! Exercise the pinned application's real scheduling/checkpoint embedding.
use omoikane::{
    dom::NodeHandle,
    js::{JsRuntime, SandboxConfig},
};
use std::{
    env, fs,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("build this worker in release mode".into());
    }
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: omoikane-async-worker WORKLOAD_JSON SAMPLES".into());
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
    let mut runtime = JsRuntime::with_document_and_sandbox(
        NodeHandle::document(),
        SandboxConfig {
            max_loop_iterations: 100_000_000,
            timeout: Duration::from_secs(60),
        },
    )
    .map_err(|e| format!("startup: {e}"))?;
    let startup_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    runtime.eval(setup).map_err(|e| format!("setup: {e}"))?;
    let setup_ns = start.elapsed().as_nanos();
    let mut run = || -> Result<u64, Box<dyn std::error::Error>> {
        let start = Instant::now();
        let scheduled = runtime
            .eval(invocation)
            .map_err(|e| format!("schedule: {e}"))?;
        if scheduled.as_number().map(f64::to_bits) != Some(before.to_bits()) {
            return Err("unexpected result before checkpoint; callbacks may have run early".into());
        }
        runtime.run_jobs().map_err(|e| format!("checkpoint: {e}"))?;
        let value = runtime
            .eval(completion)
            .map_err(|e| format!("completion: {e}"))?;
        let elapsed = u64::try_from(start.elapsed().as_nanos())?;
        if value.as_number().map(f64::to_bits) != Some(expected.to_bits()) {
            return Err(format!("expected {expected} after checkpoint, got {value:?}").into());
        }
        Ok(elapsed)
    };
    let warmup_ns = run()?;
    let samples_ns = (0..samples).map(|_| run()).collect::<Result<Vec<_>, _>>()?;
    println!(
        "{}",
        serde_json::json!({
            "engine":"omoikane vendored Boa", "mode":"interpreter", "profile":"release",
            "architecture":env::consts::ARCH,
            "scope":"parse+evaluate scheduling; drain jobs; parse+evaluate completion; setup excluded",
            "expected":expected,"before_drain_expected":before,"validated_passes":samples+1,
            "startup_ns":startup_ns,"setup_ns":setup_ns,"warmup_ns":warmup_ns,"samples_ns":samples_ns,
            "drained_jobs":null,"drained_jobs_note":"Public application run_jobs returns unit; completion oracle validates queued effects",
            "sandbox":{"timeout_seconds":60,"max_loop_iterations":100_000_000}
        })
    );
    Ok(())
}
