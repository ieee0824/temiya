//! Actual CLI tests. Authored cases use unchanged vendored Test262 harness;
//! original upstream case qualification is performed by the pinned tests.
#![cfg(target_os = "linux")]
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "temiya-async-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(dir.join("harness")).unwrap();
        fs::create_dir(dir.join("test")).unwrap();
        for (name, text) in [
            (
                "assert.js",
                include_str!("fixtures/test262-async/harness/assert.js"),
            ),
            (
                "sta.js",
                include_str!("fixtures/test262-async/harness/sta.js"),
            ),
            (
                "doneprintHandle.js",
                include_str!("fixtures/test262-async/harness/doneprintHandle.js"),
            ),
        ] {
            fs::write(dir.join("harness").join(name), text).unwrap();
        }
        Self(dir)
    }
    fn write(&self, path: &str, source: &str) {
        fs::write(self.0.join(path), source).unwrap();
    }
    fn run(&self, flags: &str, source: &str, extra: &[&str]) -> (Output, Value, Duration) {
        self.write(
            "test/case.js",
            &format!("/*---\nflags: [{flags}]\n---*/\n{source}"),
        );
        self.cli(extra)
    }
    fn cli(&self, extra: &[&str]) -> (Output, Value, Duration) {
        let started = Instant::now();
        let output = Command::new("timeout")
            .args([
                "--kill-after=1s",
                "5s",
                env!("CARGO_BIN_EXE_temiya-test262"),
                "--corpus",
            ])
            .arg(&self.0)
            .args(["--timeout-ms", "200"])
            .args(extra)
            .arg("test/case.js")
            .output()
            .unwrap();
        let report = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
        (output, report, started.elapsed())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn expect(output: &Output, report: &Value, code: i32, kind: &str) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{report}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(report["results"][0]["async_outcome"], kind, "{report}");
    assert_eq!(report["counts"]["skip"], 0);
}
#[test]
fn actual_async_completion_drains_jobs_and_preserves_child_fifo() {
    let fixture = Fixture::new();
    for backend in ["ast", "auto"] {
        for source in [
            "async function f(){await 1;$DONE();}f();",
            "async function f(){throw 7;} async function tick(){return 0;}var p=f();tick().then(function(){p.then(undefined,function(x){assert.sameValue(x,7);$DONE();});});",
            "var order='';var child=$262.createRealm();child.global.parent=$262.global;child.evalScript(\"async function tick(){return 0;}tick().then(function(){parent.order+=1;});\");async function tick(){return 0;}tick().then(function(){order+=2;assert.sameValue(order,'12');$DONE();});",
            "assert.sameValue(Object.getOwnPropertyDescriptor(this,'print').enumerable,false);$DONE(false);",
        ] {
            let (o, r, _) = fixture.run(
                "async, noStrict",
                source,
                &["--backend", backend, "--timeout-ms", "1500"],
            );
            expect(&o, &r, 0, "complete");
            assert_eq!(r["counts"], json!({"pass":1,"fail":0,"skip":0}));
        }
    }
}
#[test]
fn async_failures_and_duplicate_notifications_never_pass() {
    let fixture = Fixture::new();
    for (source, kind) in [
        ("$DONE(new TypeError());", "failure"),
        ("print('Test262:AsyncTestFailure:broken');", "failure"),
        ("async function f(){await 1;throw 7;}f();", "rejected"),
        (
            "$DONE();async function f(){await 1;throw 7;}f();",
            "rejected",
        ),
        ("$DONE();$DONE();", "duplicate-completion"),
        (
            "$DONE();async function f(){await 1;$DONE();}f();",
            "duplicate-completion",
        ),
    ] {
        // These finite scripts verify the exact failure notification/rejection.
        // Instrumented workers need the same budget as completion tests above.
        let (o, r, _) = fixture.run("async, noStrict", source, &["--timeout-ms", "1500"]);
        expect(&o, &r, 1, kind);
        assert_eq!(r["counts"]["pass"], 0);
    }
}
#[test]
fn incomplete_and_nonreturning_jobs_obey_real_deadline() {
    let fixture = Fixture::new();
    for (source, kind) in [
        ("1;", "incomplete"),
        ("while(true){}", "timeout"),
        ("async function f(){await 1;while(true){}}f();", "timeout"),
    ] {
        let (o, r, time) = fixture.run("async, noStrict", source, &[]);
        expect(&o, &r, 1, kind);
        assert_eq!(r["results"][0]["timed_out"], true);
        assert!(time >= Duration::from_millis(180), "{time:?}");
        assert!(time < Duration::from_secs(3), "{time:?}");
        assert_eq!(r["counts"], json!({"pass":0,"fail":1,"skip":0}));
    }
}
#[test]
fn raw_async_uses_print_without_loading_or_rewriting_harness() {
    let fixture = Fixture::new();
    fixture.write("harness/doneprintHandle.js", "throw 1;");
    let(o,r,_)=fixture.run("async, raw","if(typeof assert!=='undefined'||typeof $DONE!=='undefined')throw 42;print('Test262:AsyncTestComplete');",&[]);
    expect(&o, &r, 0, "complete");
}
#[test]
fn original_done_harness_precedes_includes_and_setup_cannot_complete_test() {
    let fixture = Fixture::new();
    fixture.write(
        "harness/check.js",
        "assert.sameValue(typeof $DONE,'function');var included=42;",
    );
    fixture.write("test/case.js","/*---\nflags: [async, noStrict]\nincludes: [check.js]\n---*/\nassert.sameValue(included,42);$DONE();");
    let (o, r, _) = fixture.cli(&[]);
    expect(&o, &r, 0, "complete");
    fixture.write("harness/check.js", "$DONE();");
    let (o, r, _) = fixture.cli(&[]);
    assert_eq!(o.status.code(), Some(1), "{r}");
    assert_eq!(r["results"][0]["observed_failure"]["type"], "HarnessError");
}
#[test]
fn protocol_errors_and_getters_cannot_satisfy_negative_cases() {
    let fixture = Fixture::new();
    for (source, ty) in [
        ("$DONE(new TypeError());", "TypeError"),
        ("$DONE();$DONE();", "AsyncDuplicateCompletion"),
        (
            "throw {constructor:TypeError,get message(){print('Test262:AsyncTestComplete');return '';}};",
            "TypeError",
        ),
    ] {
        fixture.write("test/case.js",&format!("/*---\nflags: [async, noStrict]\nnegative:\n  phase: runtime\n  type: {ty}\n---*/\n{source}"));
        let (o, r, _) = fixture.cli(&[]);
        assert_eq!(o.status.code(), Some(1), "{r}");
        assert_eq!(r["counts"]["pass"], 0);
    }
}
#[test]
fn async_parse_negative_needs_no_completion_and_policy_skip_precedes_worker() {
    let fixture = Fixture::new();
    fixture.write("test/case.js","/*---\nflags: [async, noStrict]\nnegative:\n  phase: parse\n  type: SyntaxError\n---*/\nlet = ;");
    let (o, r, _) = fixture.cli(&[]);
    assert_eq!(o.status.code(), Some(0), "{r}");
    assert_eq!(r["results"][0]["observed_failure"]["phase"], "Parse");
    fixture.write(
        "test/case.js",
        "/*---\nflags: [async, noStrict]\n---*/\nwhile(true){}",
    );
    fixture.write("skip.json",&json!({"revision":temiya::test262::TEST262_REVISION,"skips":[{"path":"test/case.js","reason":"policy","detail":"test policy"}]}).to_string());
    let manifest = fixture.0.join("skip.json");
    let (o, r, _) = fixture.cli(&[
        "--manifest",
        manifest.to_str().unwrap(),
        "--timeout-ms",
        "1",
    ]);
    assert_eq!(o.status.code(), Some(3), "{r}");
    assert_eq!(r["counts"], json!({"pass":0,"fail":0,"skip":1}));
}

#[test]
fn oversized_worker_reports_fail_without_blocking_the_parent_pipe() {
    let fixture = Fixture::new();
    let source = format!(
        "$DONE();throw {{constructor:TypeError,message:'{}'}};",
        "x".repeat(1024 * 1024 + 128)
    );
    let (o, r, _) = fixture.run("async, noStrict", &source, &["--timeout-ms", "3000"]);
    expect(&o, &r, 1, "worker-error");
    assert!(
        r["results"][0]["detail"]
            .as_str()
            .unwrap()
            .contains("protocol limit"),
        "{r}"
    );
}

#[test]
fn cli_async_runs_both_required_variants_and_rejects_unbounded_configuration() {
    let fixture = Fixture::new();
    let (o, r, _) = fixture.run(
        "async",
        "async function f(){await 1;$DONE();}f();",
        &["--timeout-ms", "1500"],
    );
    assert_eq!(o.status.code(), Some(0), "{r}");
    assert_eq!(r["counts"], json!({"pass":2,"fail":0,"skip":0}));
    for value in ["0", "120001", "-1", "not-a-number"] {
        let (o, _, _) = fixture.cli(&["--timeout-ms", value]);
        assert_eq!(o.status.code(), Some(2));
    }
}

#[test]
#[ignore = "requires immutable Test262 corpus; original async cases and harness identity"]
fn pinned_async_cases_use_original_done_harness() {
    let corpus = std::env::var("TEST262_DIR").expect("set TEST262_DIR");
    for (name, source) in [
        (
            "assert.js",
            include_str!("fixtures/test262-async/harness/assert.js"),
        ),
        (
            "sta.js",
            include_str!("fixtures/test262-async/harness/sta.js"),
        ),
        (
            "doneprintHandle.js",
            include_str!("fixtures/test262-async/harness/doneprintHandle.js"),
        ),
    ] {
        let original = Command::new("git")
            .arg("-C")
            .arg(&corpus)
            .args([
                "show",
                &format!("{}:harness/{name}", temiya::test262::TEST262_REVISION),
            ])
            .output()
            .unwrap();
        assert!(original.status.success());
        assert_eq!(
            original.stdout,
            source.as_bytes(),
            "vendored {name} must remain unmodified"
        );
    }
    for backend in ["ast", "auto"] {
        let output = Command::new("timeout")
            .args([
                "--kill-after=1s",
                "20s",
                env!("CARGO_BIN_EXE_temiya-test262"),
                "--corpus",
            ])
            .arg(&corpus)
            .args([
                "--pinned",
                "--backend",
                backend,
                "--timeout-ms",
                "2000",
                "test/language/statements/async-function/evaluation-body-that-returns.js",
                "test/language/statements/async-function/evaluation-body-that-throws.js",
            ])
            .output()
            .unwrap();
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(output.status.code(), Some(0), "{report}");
        assert_eq!(report["corpus_revision"], temiya::test262::TEST262_REVISION);
        assert_eq!(report["counts"], json!({"pass":4,"fail":0,"skip":0}));
        assert!(
            report["results"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["async_outcome"] == "complete")
        );
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    }
}
