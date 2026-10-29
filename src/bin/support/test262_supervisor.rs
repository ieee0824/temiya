//! Bounded variant execution. The Engine stays in a child process; one
//! non-returning Script/job cannot disable the parent's wall-clock deadline.
use crate::resources;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Worker(Child);
impl std::ops::Deref for Worker {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for Worker {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn capture(
    mut stream: impl Read + Send + 'static,
    limit: usize,
) -> thread::JoinHandle<(Vec<u8>, bool)> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut overflow = false;
        let mut buffer = [0u8; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let keep = count.min(limit.saturating_sub(bytes.len()));
                    bytes.extend_from_slice(&buffer[..keep]);
                    overflow |= keep != count;
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    overflow = true;
                    break;
                }
            }
        }
        (bytes, overflow)
    })
}
fn failure(
    path: &str,
    variant: &str,
    kind: &str,
    detail: String,
    outcome: &str,
    timeout: bool,
) -> Value {
    json!({"path":path,"variant":variant,"status":"fail","reason":if timeout{Some("timeout")}else{None},"detail":detail,"diagnostic":null,"observed_failure":{"phase":"Runtime","type":kind},"async_outcome":outcome,"timed_out":timeout})
}

pub struct Request<'a> {
    pub corpus: &'a Path,
    pub pinned: bool,
    pub backend: &'a str,
    pub path: &'a str,
    pub variant: &'a str,
    pub source: &'a str,
    pub manifest: &'a str,
    pub timeout_ms: u64,
    pub limits: resources::Limits,
    pub async_mode: bool,
}

pub fn run(request: Request<'_>) -> Value {
    let Request {
        corpus,
        pinned,
        backend,
        path,
        variant,
        source,
        manifest,
        timeout_ms,
        limits,
        async_mode,
    } = request;
    let started = Instant::now();
    let budget = Duration::from_millis(timeout_ms);
    let mut exit_details = Value::Null;
    let mut peak_rss_kib = 0;
    let mut limits_applied = false;
    let mut execute = || -> Result<Value, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut command = Command::new(executable);
        command.arg("--corpus").arg(corpus).args([
            "--backend",
            backend,
            "--internal-worker",
            variant,
            "--timeout-ms",
            &timeout_ms.to_string(),
            "--memory-mb",
            &limits.memory_mb.to_string(),
            "--stack-kb",
            &limits.stack_kb.to_string(),
        ]);
        if pinned {
            command.arg("--pinned");
        }
        command
            .arg("--")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        limits
            .configure(&mut command)
            .map_err(|e| format!("worker resource limits: {e}"))?;
        let mut child = Worker(command.spawn().map_err(|e| e.to_string())?);
        limits_applied = true;
        let output = capture(
            child.stdout.take().ok_or("missing worker stdout")?,
            1024 * 1024,
        );
        let errors = capture(
            child.stderr.take().ok_or("missing worker stderr")?,
            64 * 1024,
        );
        let mut input = child.stdin.take().ok_or("missing worker stdin")?;
        let request = serde_json::to_vec(&json!({"source":source,"manifest":manifest}))
            .map_err(|e| e.to_string())?;
        let writer = thread::spawn(move || input.write_all(&request));
        let mut expired = false;
        let mut memory_exceeded = false;
        let status = loop {
            let status = child.try_wait().map_err(|e| e.to_string())?;
            if let Some(rss) = resources::resident_kib(child.id()) {
                peak_rss_kib = peak_rss_kib.max(rss);
                if rss > limits.memory_bytes() / 1024 {
                    memory_exceeded = true;
                    if status.is_none() {
                        let _ = child.kill();
                    }
                    break child.wait().map_err(|e| e.to_string())?;
                }
            }
            if started.elapsed() >= budget {
                expired = true;
                if status.is_none() {
                    let _ = child.kill();
                }
                break child.wait().map_err(|e| e.to_string())?;
            }
            if let Some(status) = status {
                break status;
            }
            thread::sleep(Duration::from_millis(2).min(budget.saturating_sub(started.elapsed())));
        };
        let _ = writer.join();
        let (output, overflow) = output.join().map_err(|_| "worker stdout reader panicked")?;
        let (errors, error_overflow) =
            errors.join().map_err(|_| "worker stderr reader panicked")?;
        exit_details = json!({"code":status.code(),"signal":resources::signal(status)});
        let diagnostic = String::from_utf8_lossy(&errors);
        if memory_exceeded {
            return Ok(failure(
                path,
                variant,
                "OutOfMemory",
                format!("worker RSS exceeded {} MiB", limits.memory_mb),
                "worker-error",
                false,
            ));
        }
        if expired || status.code() == Some(124) {
            return Ok(failure(
                path,
                variant,
                if async_mode {
                    "AsyncTimeout"
                } else {
                    "ExecutionTimeout"
                },
                format!("worker exceeded {timeout_ms} ms"),
                "timeout",
                true,
            ));
        }
        // ASan exits with code 1 rather than Rust's fatal signal. Only an
        // instrumented build with no worker JSON can use this diagnostic path.
        let sanitizer_failure =
            if resources::ADDRESS_SANITIZED && output.is_empty() && status.code() == Some(1) {
                if diagnostic.contains("ERROR: AddressSanitizer: stack-overflow")
                    && diagnostic.contains("SUMMARY: AddressSanitizer: stack-overflow")
                {
                    Some("StackOverflow")
                } else if diagnostic.contains("ERROR: AddressSanitizer: allocator is out of memory")
                    && diagnostic.contains("SUMMARY: AddressSanitizer: out-of-memory")
                {
                    Some("OutOfMemory")
                } else {
                    None
                }
            } else {
                None
            };
        if sanitizer_failure.is_some() || !matches!(status.code(), Some(0 | 1 | 3)) {
            // Signals alone do not establish memory or stack exhaustion. Rust's
            // fatal diagnostic must accompany the abnormal process termination.
            let kind = sanitizer_failure.unwrap_or_else(|| {
                if diagnostic.contains("memory allocation of ") && diagnostic.contains(" failed") {
                    "OutOfMemory"
                } else if diagnostic.contains("has overflowed its stack") {
                    "StackOverflow"
                } else {
                    "WorkerCrash"
                }
            });
            return Ok(failure(
                path,
                variant,
                kind,
                format!("worker terminated ({exit_details}): {diagnostic}"),
                "worker-error",
                false,
            ));
        }
        if overflow || error_overflow {
            return Err("worker output exceeded the protocol limit".into());
        }
        let report: Value = serde_json::from_slice(&output).map_err(|e| {
            format!(
                "invalid worker report: {e}; {}",
                String::from_utf8_lossy(&errors)
            )
        })?;
        if report["schema"] != 1 {
            return Err("invalid worker schema".into());
        }
        let expected_backend = match backend {
            "ast" => "Ast",
            "bytecode" => "Bytecode",
            _ => "Auto",
        };
        let expected_manifest: Value = serde_json::from_str(manifest).map_err(|e| e.to_string())?;
        if report["backend"] != expected_backend
            || report["manifest_revision"] != expected_manifest["revision"]
            || (pinned && report["corpus_revision"] != temiya::test262::TEST262_REVISION)
            || (!pinned && !report["corpus_revision"].is_null())
        {
            return Err("worker configuration identity mismatch".into());
        }
        let results = report["results"]
            .as_array()
            .ok_or("missing worker results")?;
        if results.len() != 1 || results[0]["path"] != path || results[0]["variant"] != variant {
            return Err("worker result identity mismatch".into());
        }
        let mut record = results[0].clone();
        let kind = record["status"].as_str().ok_or("missing worker status")?;
        if async_mode
            && kind == "pass"
            && record["async_outcome"] != "complete"
            && !matches!(
                record["observed_failure"]["phase"].as_str(),
                Some("Parse" | "Resolution")
            )
        {
            return Err("async worker passed without completion".into());
        }
        let expected = if kind == "pass" {
            0
        } else if kind == "skip" {
            3
        } else if kind == "fail" {
            1
        } else {
            return Err("invalid worker status".into());
        };
        if status.code() != Some(expected) {
            return Err("worker exit/result mismatch".into());
        }
        if record["async_outcome"] == "incomplete" {
            thread::sleep(budget.saturating_sub(started.elapsed()));
            record["timed_out"] = json!(true);
            record["reason"] = json!("timeout");
            record["detail"] = json!(format!(
                "async completion remained incomplete for {timeout_ms} ms"
            ));
        }
        Ok(record)
    };
    let mut record = match execute() {
        Ok(record) => record,
        Err(error) => failure(path, variant, "HarnessError", error, "worker-error", false),
    };
    if !async_mode {
        record["async_outcome"] = Value::Null;
    }
    record["worker_exit"] = exit_details;
    record["resources"] = json!({"applied":limits_applied,"timeout_ms":timeout_ms,"memory_limit_bytes":limits.memory_bytes(),"address_space_limit_bytes":limits_applied.then(|| limits.address_space_bytes()).flatten(),"address_sanitizer":resources::ADDRESS_SANITIZED,"stack_limit_bytes":limits.stack_bytes(),"peak_rss_kib":peak_rss_kib});
    record
}
