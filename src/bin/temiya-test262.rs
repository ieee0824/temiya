//! Test262 command-line adapter over the public Engine.
use serde_json::{Value, json};
use std::{env, fs, io::Read, path::PathBuf, process::ExitCode, time::Duration};
#[path = "support/test262_engine.rs"]
mod engine;
#[path = "support/test262_supervisor.rs"]
mod supervisor;
use engine::{Corpus, EngineTestResult, parse_case, run_with_engine};
use temiya::{
    embedding::ScriptBackend,
    test262::{Manifest, ResultStatus, TestVariant},
};

const HELP: &str = "Usage: temiya-test262 --corpus DIR [--pinned] [--backend auto|ast|bytecode]
                      [--timeout-ms 10000] [--manifest FILE] [--output FILE] TEST_PATH [...]

TEST_PATH is corpus-relative. Each test/variant gets a fresh Engine and Realm.
--pinned reads every source/harness file from the fixed Git commit.
Async variants use separate workers with a finite wall-clock deadline (1..120000 ms).
JSON distinguishes completion, rejection, duplicate notification, incomplete and timeout.
Exit: 0 success, 1 test/setup failure, 2 usage error, 3 all variants skipped.";
struct Options {
    corpus: PathBuf,
    pinned: bool,
    backend: ScriptBackend,
    manifest: Option<PathBuf>,
    output: Option<PathBuf>,
    tests: Vec<String>,
    timeout_ms: u64,
    worker: Option<TestVariant>,
}
fn parse() -> Result<Option<Options>, String> {
    let mut args = env::args().skip(1);
    let (mut corpus, mut manifest, mut output) = (None, None, None);
    let mut pinned = false;
    let mut backend = ScriptBackend::Auto;
    let mut tests = Vec::new();
    let mut timeout_ms = 10_000;
    let mut worker = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--pinned" => pinned = true,
            "--corpus" => {
                corpus = Some(PathBuf::from(args.next().ok_or("missing --corpus value")?))
            }
            "--manifest" => {
                manifest = Some(PathBuf::from(
                    args.next().ok_or("missing --manifest value")?,
                ))
            }
            "--output" => {
                output = Some(PathBuf::from(args.next().ok_or("missing --output value")?))
            }
            "--backend" => {
                backend = match args.next().as_deref() {
                    Some("ast") => ScriptBackend::Ast,
                    Some("bytecode") => ScriptBackend::Bytecode,
                    Some("auto") => ScriptBackend::Auto,
                    _ => return Err("--backend requires auto, ast or bytecode".into()),
                }
            }
            "--timeout-ms" => {
                timeout_ms = args
                    .next()
                    .ok_or("missing --timeout-ms value")?
                    .parse()
                    .map_err(|_| "invalid --timeout-ms")?;
                if !(1..=120_000).contains(&timeout_ms) {
                    return Err("--timeout-ms must be 1..120000".into());
                }
            }
            "--internal-worker" => {
                worker = Some(match args.next().as_deref() {
                    Some("Sloppy") => TestVariant::Sloppy,
                    Some("Strict") => TestVariant::Strict,
                    Some("Module") => TestVariant::Module,
                    _ => return Err("invalid worker variant".into()),
                })
            }
            "--" => {
                tests.extend(args);
                break;
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}")),
            _ => tests.push(arg),
        }
    }
    if tests.is_empty() {
        return Err("at least one TEST_PATH is required".into());
    }
    if worker.is_some() && tests.len() != 1 {
        return Err("a worker requires one TEST_PATH".into());
    }
    Ok(Some(Options {
        corpus: corpus.ok_or("--corpus is required")?,
        pinned,
        backend,
        manifest,
        output,
        tests,
        timeout_ms,
        worker,
    }))
}
fn record(execution: EngineTestResult) -> Value {
    let result = execution.result;
    let (status, reason, detail) = match result.status {
        ResultStatus::Passed => ("pass", None, None),
        ResultStatus::Failed(detail) => ("fail", None, Some(detail)),
        ResultStatus::Skipped { reason, detail } => ("skip", Some(reason), Some(detail)),
    };
    let failure = execution
        .observed_failure
        .map(|e| json!({"phase":format!("{:?}",e.phase),"type":e.error_type}));
    json!({"path":result.path,"variant":format!("{:?}",result.variant),"status":status,"reason":reason,"detail":detail,"diagnostic":execution.diagnostic,"observed_failure":failure,"async_outcome":execution.async_outcome})
}
fn execute(options: Options) -> Result<u8, String> {
    let corpus = if options.pinned {
        Corpus::pinned(&options.corpus)
    } else {
        Corpus::directory(&options.corpus)
    }
    .map_err(|e| e.to_string())?;
    let request: Option<Value> = if options.worker.is_some() {
        let mut text = String::new();
        std::io::stdin()
            .take(32 * 1024 * 1024)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        Some(serde_json::from_str(&text).map_err(|e| format!("worker input: {e}"))?)
    } else {
        None
    };
    let manifest_text = if let Some(request) = &request {
        request["manifest"]
            .as_str()
            .ok_or("missing worker manifest")?
            .to_owned()
    } else {
        match &options.manifest {
            Some(path) => fs::read_to_string(path).map_err(|e| e.to_string())?,
            None => include_str!("../../test262/manifest.json").into(),
        }
    };
    let manifest = Manifest::from_json(&manifest_text).map_err(|e| e.to_string())?;
    if corpus
        .revision()
        .is_some_and(|revision| revision != manifest.revision)
    {
        return Err("manifest revision differs from consumed pinned corpus".into());
    }
    let mut records = Vec::new();
    for path in options.tests {
        let test = (|| {
            let source = if let Some(request) = &request {
                let source = request["source"]
                    .as_str()
                    .ok_or_else(|| temiya::test262::HarnessError("missing worker source".into()))?
                    .to_owned();
                if options.pinned && corpus.read(&path)? != source {
                    return Err(temiya::test262::HarnessError(
                        "pinned worker source mismatch".into(),
                    ));
                }
                source
            } else {
                corpus.read(&path)?
            };
            parse_case(&path, source)
        })();
        let test = match test {
            Ok(test) => test,
            Err(error) => {
                records.push(json!({"path":path,"variant":null,"status":"fail","stage":"setup","detail":error.to_string()}));
                continue;
            }
        };
        if let Some(variant) = options.worker {
            records.extend(
                engine::run_worker_variant(&test, &manifest, &corpus, options.backend, variant)
                    .into_iter()
                    .map(record),
            );
        } else if test.metadata.flags.iter().any(|flag| flag == "async") {
            if let Some(gated) = engine::preflight(&test, &manifest, true) {
                records.extend(gated.into_iter().map(record));
                continue;
            }
            let backend = match options.backend {
                ScriptBackend::Ast => "ast",
                ScriptBackend::Bytecode => "bytecode",
                ScriptBackend::Auto => "auto",
            };
            for variant in test.variants() {
                records.push(supervisor::run(supervisor::Request {
                    corpus: &options.corpus,
                    pinned: options.pinned,
                    backend,
                    path: &path,
                    variant: &format!("{variant:?}"),
                    source: &test.source,
                    manifest: &manifest_text,
                    timeout_ms: options.timeout_ms,
                }));
            }
        } else {
            records.extend(
                run_with_engine(&test, &manifest, &corpus, options.backend)
                    .into_iter()
                    .map(record),
            );
        }
    }
    let passed = records.iter().filter(|r| r["status"] == "pass").count();
    let failed = records.iter().filter(|r| r["status"] == "fail").count();
    let skipped = records.iter().filter(|r| r["status"] == "skip").count();
    let report = json!({"schema":1,"corpus_revision":corpus.revision(),"manifest_revision":manifest.revision,"backend":format!("{:?}",options.backend),"counts":{"pass":passed,"fail":failed,"skip":skipped},"results":records});
    let report = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(path) = options.output {
        fs::write(path, &report).map_err(|e| e.to_string())?;
    }
    println!("{report}");
    Ok(if failed > 0 {
        1
    } else if passed == 0 {
        3
    } else {
        0
    })
}
fn main() -> ExitCode {
    match parse() {
        Ok(None) => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}\n\n{HELP}");
            ExitCode::from(2)
        }
        Ok(Some(options)) => {
            if options.worker.is_some() {
                let timeout = options.timeout_ms;
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(timeout));
                    std::process::exit(124);
                });
            }
            match execute(options) {
                Ok(code) => ExitCode::from(code),
                Err(error) => {
                    eprintln!("temiya-test262: {error}");
                    ExitCode::from(1)
                }
            }
        }
    }
}
