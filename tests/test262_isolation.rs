//! Real CLI process boundaries; authored cases are not upstream qualification.
#![cfg(target_os = "linux")]
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Corpus(PathBuf);
impl Corpus {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "temiya-isolation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("test")).unwrap();
        fs::create_dir(root.join("harness")).unwrap();
        for (name, text) in [
            (
                "assert.js",
                include_str!("fixtures/test262-async/harness/assert.js"),
            ),
            (
                "sta.js",
                include_str!("fixtures/test262-async/harness/sta.js"),
            ),
        ] {
            fs::write(root.join("harness").join(name), text).unwrap();
        }
        Self(root)
    }

    fn write(&self, name: &str, body: &str) {
        fs::write(
            self.0.join("test").join(name),
            format!("/*---\nflags: [raw]\n---*/\n{body}"),
        )
        .unwrap();
    }

    fn write_negative(&self, name: &str, body: &str) {
        fs::write(self.0.join("test").join(name),format!("/*---\nflags: [raw]\nnegative:\n  phase: runtime\n  type: RangeError\n---*/\n{body}")).unwrap();
    }

    fn run(&self, backend: &str, paths: &[&str], extra: &[&str]) -> (i32, Value) {
        let output = Command::new("timeout")
            .args([
                "--kill-after=1s",
                "12s",
                env!("CARGO_BIN_EXE_temiya-test262"),
                "--corpus",
            ])
            .arg(&self.0)
            .args([
                "--backend",
                backend,
                "--timeout-ms",
                "500",
                "--memory-mb",
                "128",
                "--stack-kb",
                "2048",
            ])
            .args(extra)
            .args(paths)
            .output()
            .unwrap();
        let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "missing CLI report: {e}; status {:?}; stderr {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )
        });
        (output.status.code().unwrap(), report)
    }
}
impl Drop for Corpus {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn synchronous_loop_times_out_and_the_next_test_still_runs() {
    let corpus = Corpus::new();
    corpus.write("loop.js", "while (true) {}");
    corpus.write("after.js", "42;");
    for backend in ["ast", "auto", "bytecode"] {
        let (code, r) = corpus.run(backend, &["test/loop.js", "test/after.js"], &[]);
        assert_eq!(code, 1);
        assert_eq!(r["counts"]["fail"], 1);
        assert_eq!(r["counts"]["pass"], 1);
        let failed = &r["results"][0];
        assert_eq!(failed["observed_failure"]["type"], "ExecutionTimeout");
        assert_eq!(failed["timed_out"], true);
        assert!(failed["async_outcome"].is_null());
        assert_eq!(failed["resources"]["applied"], true);
        assert_eq!(r["results"][1]["status"], "pass");
    }
}

#[test]
fn each_synchronous_test_gets_a_fresh_global() {
    let corpus = Corpus::new();
    corpus.write("first.js", "var leaked = 42;");
    corpus.write("second.js", "if (typeof leaked !== 'undefined') throw 1;");
    for backend in ["ast", "auto", "bytecode"] {
        let (code, r) = corpus.run(backend, &["test/first.js", "test/second.js"], &[]);
        assert_eq!(code, 0);
        assert_eq!(r["counts"]["pass"], 2);
        for result in r["results"].as_array().unwrap() {
            assert_eq!(result["worker_exit"]["code"], 0);
            assert_eq!(result["resources"]["applied"], true);
            assert!(result["async_outcome"].is_null());
        }
    }
}

#[test]
fn javascript_range_errors_keep_their_language_negative_semantics() {
    let corpus = Corpus::new();
    corpus.write_negative(
        "depth.js",
        "function recurse() { return 1 + recurse(); } recurse();",
    );
    corpus.write_negative("guard.js", "new ArrayBuffer(100000001);");
    corpus.write_negative(
        "message.js",
        "throw new RangeError('memory allocation of 7 failed');",
    );
    for backend in ["ast", "auto", "bytecode"] {
        let (code, r) = corpus.run(
            backend,
            &["test/depth.js", "test/guard.js", "test/message.js"],
            &["--stack-kb", "8192", "--timeout-ms", "1500"],
        );
        assert_eq!(code, 0, "{r}");
        assert_eq!(r["counts"]["pass"], 3, "{r}");
        for result in r["results"].as_array().unwrap() {
            assert_eq!(result["observed_failure"]["type"], "RangeError", "{r}");
            assert_eq!(result["worker_exit"]["code"], 0, "{r}");
        }
    }
}

#[test]
fn pending_job_after_abrupt_script_does_not_run_in_the_next_test() {
    let corpus = Corpus::new();
    corpus.write_negative("pending.js","async function tick(){return 1;}tick().then(function(){while(true){}});throw new RangeError();");
    corpus.write("after.js", "42;");
    for backend in ["ast", "auto"] {
        let (code, r) = corpus.run(
            backend,
            &["test/pending.js", "test/after.js"],
            &["--timeout-ms", "1500"],
        );
        assert_eq!(code, 0, "{r}");
        assert_eq!(r["counts"]["pass"], 2, "{r}");
        assert_eq!(
            r["results"][0]["observed_failure"]["type"], "RangeError",
            "{r}"
        );
        assert_eq!(r["results"][1]["status"], "pass", "{r}");
    }
}

#[test]
fn stack_and_allocation_exhaustion_are_distinct_and_never_satisfy_negatives() {
    let corpus = Corpus::new();
    // Exercise actual expression traversal without disabling the Engine's
    // ordinary call-depth guard.
    corpus.write_negative("stack.js", &format!("{};", vec!["1"; 20_000].join("+")));
    // Each request is below the Engine's ArrayBuffer size guard. Retaining the
    // buffers forces real allocation exhaustion within the disposable worker.
    corpus.write_negative(
        "oom.js",
        "var buffers=[]; while(true) { buffers.push(new ArrayBuffer(67108864)); }",
    );
    corpus.write("after.js", "42;");
    for backend in ["ast", "auto", "bytecode"] {
        for (path, kind) in [
            ("test/stack.js", "StackOverflow"),
            ("test/oom.js", "OutOfMemory"),
        ] {
            let stack = if kind == "StackOverflow" {
                "512"
            } else {
                "2048"
            };
            let (code, r) = corpus.run(
                backend,
                &[path, "test/after.js"],
                &["--timeout-ms", "5000", "--stack-kb", stack],
            );
            assert_eq!(code, 1, "{r}");
            assert_eq!(r["counts"]["fail"], 1, "{r}");
            assert_eq!(r["counts"]["pass"], 1, "{r}");
            assert_eq!(r["results"][0]["observed_failure"]["type"], kind, "{r}");
            assert_eq!(r["results"][0]["timed_out"], false, "{r}");
            assert_eq!(r["results"][1]["status"], "pass", "{r}");
        }
    }
}

#[test]
fn externally_signalled_worker_crashes_do_not_kill_the_suite_or_imply_oom() {
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };
    let corpus = Corpus::new();
    corpus.write("crash.js", "while (true) {}");
    corpus.write("after.js", "42;");
    // SIGABRT also accompanies Rust allocation/stack failures; without their
    // diagnostics it is a generic crash. SIGKILL alone must not imply OOM.
    for signal in [libc::SIGABRT, libc::SIGKILL] {
        let mut parent = Command::new("timeout")
            .args([
                "--kill-after=1s",
                "12s",
                env!("CARGO_BIN_EXE_temiya-test262"),
                "--corpus",
            ])
            .arg(&corpus.0)
            .args(["--timeout-ms", "5000", "test/crash.js", "test/after.js"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let worker = loop {
            // Find this fixture's internal worker, not another test process.
            let found = fs::read_dir("/proc")
                .unwrap()
                .filter_map(Result::ok)
                .find_map(|entry| {
                    let pid = entry.file_name().to_str()?.parse::<i32>().ok()?;
                    let cmd = fs::read(entry.path().join("cmdline")).ok()?;
                    let cmd = String::from_utf8_lossy(&cmd);
                    (cmd.contains("--internal-worker")
                        && cmd.contains(corpus.0.to_str().unwrap())
                        && cmd.contains("test/crash.js"))
                    .then_some(pid)
                });
            if let Some(pid) = found {
                break pid;
            }
            if started.elapsed() > Duration::from_secs(3) {
                let _ = parent.kill();
                let _ = parent.wait();
                panic!("internal CLI worker did not start");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let limits = fs::read_to_string(format!("/proc/{worker}/limits")).unwrap();
        for (name, value) in [
            ("Max address space", "536870912"),
            ("Max stack size", "8388608"),
            ("Max core file size", "0"),
        ] {
            let line = limits.lines().find(|line| line.starts_with(name)).unwrap();
            let tail: Vec<_> = line.split_whitespace().rev().take(3).collect();
            assert_eq!(tail, vec!["bytes", value, value], "{line}");
        }
        // SAFETY: PID was read from this test fixture's spawned internal CLI
        // worker. The signal exercises actual process death without a fake JS
        // crash primitive. Its finite worker deadline also bounds cleanup.
        assert_eq!(unsafe { libc::kill(worker, signal) }, 0);
        let output = parent.wait_with_output().unwrap();
        let r: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&output.stderr)));
        assert_eq!(output.status.code(), Some(1), "{r}");
        assert_eq!(
            r["results"][0]["observed_failure"]["type"], "WorkerCrash",
            "{r}"
        );
        assert_eq!(r["results"][0]["worker_exit"]["signal"], signal, "{r}");
        assert_eq!(r["results"][1]["status"], "pass", "{r}");
    }
}
