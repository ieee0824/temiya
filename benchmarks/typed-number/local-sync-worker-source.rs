//! Diagnostic worker using the application's public JS embedding, not a raw
//! Boa Context. Build through scripts/build-omoikane-worker.mjs.
use std::{env, fs, time::{Duration, Instant}};

use omoikane::{dom::NodeHandle, js::{JsRuntime, SandboxConfig}};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("build this worker in release mode".into());
    }
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: omoikane-workload-worker WORKLOAD_JSON SAMPLES".into());
    }
    let samples: usize = args[1].parse()?;
    if !(1..=1000).contains(&samples) { return Err("samples must be in 1..=1000".into()); }
    let workload: serde_json::Value = serde_json::from_str(&fs::read_to_string(&args[0])?)?;
    let setup = workload["setup"].as_str().ok_or("missing setup")?;
    let invocation = workload["invocation"].as_str().ok_or("missing invocation")?;
    let expected = workload["expected"].as_f64().filter(|n| n.is_finite()).ok_or("invalid expected result")?;
    let start = Instant::now();
    let mut runtime = JsRuntime::with_document_and_sandbox(
        NodeHandle::document(),
        SandboxConfig { max_loop_iterations: 100_000_000, timeout: Duration::from_secs(60) },
    ).map_err(|e| format!("startup: {e}"))?;
    let startup_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    runtime.eval(setup).map_err(|e| format!("setup: {e}"))?;
    let setup_ns = start.elapsed().as_nanos();
    let mut evaluate = || -> Result<u64, Box<dyn std::error::Error>> {
        let start = Instant::now();
        let value = runtime.eval(invocation).map_err(|e| format!("invocation: {e}"))?;
        let elapsed = u64::try_from(start.elapsed().as_nanos())?;
        if value.as_number().map(f64::to_bits) != Some(expected.to_bits()) {
            return Err(format!("expected {expected}, got {value:?}").into());
        }
        Ok(elapsed)
    };
    let warmup_ns = evaluate()?;
    let samples_ns = (0..samples).map(|_| evaluate()).collect::<Result<Vec<_>, _>>()?;
    println!("{}", json!({
        "engine": "omoikane vendored Boa", "mode": "interpreter",
        "scope": "parse+evaluate invocation; function setup excluded",
        "profile": "release", "architecture": env::consts::ARCH,
        "expected": expected, "validated_passes": samples + 1,
        "startup_ns": startup_ns, "setup_ns": setup_ns,
        "warmup_ns": warmup_ns, "samples_ns": samples_ns,
        "sandbox": {"timeout_seconds":60, "max_loop_iterations":100_000_000},
    }));
    Ok(())
}
