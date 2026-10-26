//! Observe actual runtime retention; strict mode is the GC migration acceptance gate.
#[path = "support/ownership.rs"]
mod ownership;

fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !args.is_empty() && args != ["--require-reclamation"] {
        return Err("usage: ownership_probe [--require-reclamation]".into());
    }
    let cases = ownership::CASES
        .iter()
        .map(ownership::observe)
        .collect::<Result<Vec<_>, _>>()?;
    let ready = cases.iter().all(|case| !case.alive_after_drop);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "scope": "public Engine; globals removed, scripts/Realm/Engine and both value handles dropped",
            "collection_hook": "runtime teardown only; no Engine force-collection API exists yet",
            "reclamation_ready": ready,
            "cases": cases.iter().map(|case| serde_json::json!({
                "name": case.name,
                "alive_with_handle": case.alive_with_handle,
                "alive_with_clone": case.alive_with_clone,
                "alive_after_drop": case.alive_after_drop,
                "required_alive_after_migration": false
            })).collect::<Vec<_>>()
        }))?
    );
    Ok(if !args.is_empty() && !ready {
        std::process::ExitCode::from(2)
    } else {
        std::process::ExitCode::SUCCESS
    })
}
