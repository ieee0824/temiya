//! Metadata indexing uses the same real CLI/parser without executing test code.
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "temiya-metadata-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("test")).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, source: &str) {
        fs::write(self.0.join("test").join(name), source).unwrap();
    }
    fn run(&self, extra: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_temiya-test262"))
            .arg("--corpus")
            .arg(&self.0)
            .args(["--metadata-only", "--timeout-ms", "1"])
            .args(extra)
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn indexes_original_features_variants_and_negatives_without_engine_execution() {
    let f = Fixture::new();
    f.write("loop.js","/*---\nfeatures: ['alpha', shared]\nflags: [raw]\nincludes: [missing.js]\n---*/\nwhile(true){}");
    f.write("module.js","/*---\nfeatures: [top-level-await]\nflags: [module]\nnegative:\n  phase: resolution\n  type: SyntaxError\n---*/\nthis is deliberately invalid JavaScript");
    f.write(
        "ordinary.js",
        "/*---\nfeatures:\n  - 'beta'\n---*/\nwhile(true){}",
    );
    let output = f.run(&["test/loop.js", "test/module.js", "test/ordinary.js"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let r: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(r["kind"], "test262-metadata");
    assert_eq!(r["engine_executed"], false);
    assert!(r.get("counts").is_none());
    assert!(r.get("results").is_none());
    assert_eq!(
        r["cases"][0]["features"],
        serde_json::json!(["alpha", "shared"])
    );
    assert_eq!(r["cases"][0]["includes"], serde_json::json!(["missing.js"]));
    assert_eq!(r["cases"][0]["variants"], serde_json::json!(["Sloppy"]));
    assert_eq!(r["cases"][1]["variants"], serde_json::json!(["Module"]));
    assert_eq!(
        r["cases"][1]["negative"],
        serde_json::json!({"phase":"Resolution","type":"SyntaxError"})
    );
    assert_eq!(
        r["cases"][2]["variants"],
        serde_json::json!(["Sloppy", "Strict"])
    );
    for case in r["cases"].as_array().unwrap() {
        assert_eq!(case["parsed"], true);
        assert_eq!(case["engine_executed"], false);
        assert!(case.get("status").is_none());
    }
}
#[test]
fn missing_metadata_and_files_are_recorded_without_inventing_variants() {
    let f = Fixture::new();
    f.write("broken.js", "42;");
    let output = f.run(&["test/broken.js", "test/missing.js"]);
    assert_eq!(output.status.code(), Some(1));
    let r: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(r["cases"].as_array().unwrap().len(), 2);
    for case in r["cases"].as_array().unwrap() {
        assert_eq!(case["parsed"], false);
        assert_eq!(case["engine_executed"], false);
        assert!(case.get("variants").is_none());
        assert!(case.get("status").is_none());
    }
}
#[test]
fn metadata_mode_cannot_be_used_as_a_worker() {
    let f = Fixture::new();
    let output = f.run(&["--internal-worker", "Sloppy", "test/unused.js"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("--metadata-only cannot run as an internal worker")
    );
}

#[test]
fn block_comments_indentation_and_multiline_flow_preserve_real_execution() {
    let f = Fixture::new();
    f.write("block.js","/*---\n  description: |\n    flags: [onlyStrict]\n    features: [wrong]\n  features:\n    - alpha # comment\n    - 'shared#tag'\n  flags:\n    - raw\n  negative:\n    phase: runtime\n    type: RangeError\n---*/\nthrow new RangeError();");
    f.write("flow.js","/*---\nfeatures: [\n  alpha, # comment\n  'shared#tag'\n] # comment\nflags: [raw]\n---*/\n42;");
    f.write(
        "indentless.js",
        "/*---\nfeatures:\n- alpha\n- 'shared#tag'\nflags: [raw]\n---*/\n42;",
    );
    let output = f.run(&["test/block.js", "test/flow.js", "test/indentless.js"]);
    let r: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{r}");
    for case in r["cases"].as_array().unwrap() {
        assert_eq!(case["features"], serde_json::json!(["alpha", "shared#tag"]));
        assert_eq!(case["variants"], serde_json::json!(["Sloppy"]));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_temiya-test262"))
        .arg("--corpus")
        .arg(&f.0)
        .args(["--timeout-ms", "1500", "test/block.js"])
        .output()
        .unwrap();
    let r: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{r}");
    assert_eq!(r["counts"]["pass"], 1);
    assert_eq!(r["results"][0]["observed_failure"]["type"], "RangeError");
}

#[cfg(target_os = "linux")]
#[test]
fn baseline_uses_actual_outcomes_and_separates_skip_timeout_and_unexecuted() {
    let f = Fixture::new();
    for (name, features, body, flags) in [
        ("a", "alpha, shared", "if(40+2!==42)throw 1;", "raw"),
        ("b", "beta, shared", "throw 1;", "raw"),
        ("c", "alpha", "while(true){}", "raw"),
        ("d", "gamma", "while(true){}", "raw, unsupportedFlag"),
        ("e", "gamma", "throw 1;", "raw"),
        ("f", "", "42;", "raw"),
    ] {
        f.write(
            &format!("{name}.js"),
            &format!("/*---\nfeatures: [{features}]\nflags: [{flags}]\n---*/\n{body}"),
        );
    }
    f.write("broken.js", "42;");
    let universe: Vec<_> = ["a", "b", "c", "d", "e", "f", "broken"]
        .iter()
        .map(|n| format!("test/{n}.js"))
        .collect();
    let selection: Vec<_> = universe
        .iter()
        .filter(|p| p.as_str() != "test/e.js")
        .collect();
    fs::write(
        f.0.join("universe.json"),
        serde_json::to_vec(&universe).unwrap(),
    )
    .unwrap();
    fs::write(
        f.0.join("selection.json"),
        serde_json::to_vec(&selection).unwrap(),
    )
    .unwrap();
    for backend in ["ast", "auto"] {
        let output_dir = f.0.join(backend);
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/test262-baseline.mjs"
            ))
            .arg("--engine")
            .arg(env!("CARGO_BIN_EXE_temiya-test262"))
            .arg("--corpus")
            .arg(&f.0)
            .arg("--output")
            .arg(&output_dir)
            .arg("--universe")
            .arg(f.0.join("universe.json"))
            .arg("--selection")
            .arg(f.0.join("selection.json"))
            .args([
                "--backend",
                backend,
                "--timeout-ms",
                "500",
                "--batch-size",
                "2",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let bytes = fs::read(output_dir.join("baseline.json")).unwrap();
        let r: Value = serde_json::from_slice(&bytes).unwrap();
        for (status, count) in [
            ("total", 6),
            ("pass", 2),
            ("fail", 1),
            ("timeout", 1),
            ("skip", 1),
            ("not_run", 1),
            ("unreported", 0),
        ] {
            assert_eq!(r["totals"][status], count, "{r}");
        }
        assert_eq!(r["totals"]["pass_rate_reported_attempts"], 0.5);
        assert_eq!(r["universe"]["metadata_error_files"], 1);
        assert_eq!(r["universe"]["variant_count_exact"], false);
        assert_eq!(r["file_setup_failures"].as_array().unwrap().len(), 1);
        let features = r["features"].as_array().unwrap();
        let group = |name: &str| features.iter().find(|g| g["feature"] == name).unwrap();
        assert_eq!(group("alpha")["pass"], 1);
        assert_eq!(group("alpha")["timeout"], 1);
        assert_eq!(group("shared")["pass"], 1);
        assert_eq!(group("shared")["fail"], 1);
        assert_eq!(group("gamma")["skip"], 1);
        assert_eq!(group("gamma")["not_run"], 1);
        assert_eq!(r["untagged"]["pass"], 1);
        assert!(r["conditions"]["corpus_revision"].is_null());
        assert_eq!(r["conditions"]["engine_frozen_during_run"], true);
        assert_eq!(r["conditions"]["backend_requested"], backend);
        assert!(r["capture_errors"].as_array().unwrap().is_empty());
        let verify = || {
            Command::new("node")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/scripts/verify-test262-baseline.mjs"
                ))
                .arg(&output_dir)
                .output()
                .unwrap()
        };
        let verified = verify();
        assert!(
            verified.status.success(),
            "{}",
            String::from_utf8_lossy(&verified.stderr)
        );
        let mut corrupted = r.clone();
        corrupted["totals"]["pass"] = 999.into();
        fs::write(
            output_dir.join("baseline.json"),
            serde_json::to_vec(&corrupted).unwrap(),
        )
        .unwrap();
        assert!(
            !verify().status.success(),
            "forged pass total must not verify"
        );
        fs::write(output_dir.join("baseline.json"), &bytes).unwrap();
        let repeated = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/test262-baseline.mjs"
            ))
            .arg("--engine")
            .arg(env!("CARGO_BIN_EXE_temiya-test262"))
            .arg("--corpus")
            .arg(&f.0)
            .arg("--output")
            .arg(&output_dir)
            .arg("--universe")
            .arg(f.0.join("universe.json"))
            .output()
            .unwrap();
        assert!(!repeated.status.success());
        assert_eq!(fs::read(output_dir.join("baseline.json")).unwrap(), bytes);
    }
    fs::write(f.0.join("empty.json"), "[]").unwrap();
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/test262-baseline.mjs"
        ))
        .arg("--engine")
        .arg(env!("CARGO_BIN_EXE_temiya-test262"))
        .arg("--corpus")
        .arg(&f.0)
        .arg("--output")
        .arg(f.0.join("empty-selection"))
        .arg("--universe")
        .arg(f.0.join("universe.json"))
        .arg("--selection")
        .arg(f.0.join("empty.json"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let r: Value =
        serde_json::from_slice(&fs::read(f.0.join("empty-selection/baseline.json")).unwrap())
            .unwrap();
    assert_eq!(r["totals"]["not_run"], 6);
    assert_eq!(r["totals"]["pass"], 0);
    assert!(r["totals"]["pass_rate_reported_attempts"].is_null());
}
