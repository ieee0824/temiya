//! Test262 command-line adapter over the public Engine.

use std::{env, fs, path::PathBuf, process::ExitCode};

use serde_json::{Value, json};
#[path = "support/test262_engine.rs"]
mod engine;
use engine::{Corpus, parse_case, run_with_engine};
use temiya::{
    embedding::ScriptBackend,
    test262::{Manifest, ResultStatus},
};

const HELP: &str = "Usage: temiya-test262 --corpus DIR [--pinned] [--backend auto|ast|bytecode]
                      [--manifest FILE] [--output FILE] TEST_PATH [...]

TEST_PATH is corpus-relative. Each test/variant gets a fresh Engine and Realm.
--pinned reads every source and harness file from the repository's fixed Git
commit, ignoring working-tree edits. Directory mode claims no upstream revision.
JSON distinguishes pass/fail/skip; asynchronous completion is not yet supported.
Exit: 0 success, 1 test/setup failure, 2 usage error, 3 all variants skipped.";

struct Options {
    corpus: PathBuf,
    pinned: bool,
    backend: ScriptBackend,
    manifest: Option<PathBuf>,
    output: Option<PathBuf>,
    tests: Vec<String>,
}

fn parse() -> Result<Option<Options>, String> {
    let mut args = env::args().skip(1);
    let mut corpus = None;
    let mut pinned = false;
    let mut backend = ScriptBackend::Auto;
    let mut manifest = None;
    let mut output = None;
    let mut tests = Vec::new();
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
                    Some("auto") => ScriptBackend::Auto,
                    Some("ast") => ScriptBackend::Ast,
                    Some("bytecode") => ScriptBackend::Bytecode,
                    _ => return Err("--backend requires auto, ast or bytecode".into()),
                };
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
    Ok(Some(Options {
        corpus: corpus.ok_or("--corpus is required")?,
        pinned,
        backend,
        manifest,
        output,
        tests,
    }))
}

fn execute(options: Options) -> Result<u8, String> {
    let corpus = if options.pinned {
        Corpus::pinned(&options.corpus)
    } else {
        Corpus::directory(&options.corpus)
    }
    .map_err(|e| e.to_string())?;
    let manifest = match options.manifest {
        Some(path) => fs::read_to_string(path).map_err(|e| e.to_string())?,
        None => include_str!("../../test262/manifest.json").into(),
    };
    let manifest = Manifest::from_json(&manifest).map_err(|e| e.to_string())?;
    if corpus
        .revision()
        .is_some_and(|revision| revision != manifest.revision)
    {
        return Err("manifest revision differs from consumed pinned corpus".into());
    }
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut records = Vec::<Value>::new();
    for path in options.tests {
        let test = corpus
            .read(&path)
            .and_then(|source| parse_case(&path, source));
        let test = match test {
            Ok(test) => test,
            Err(error) => {
                failed += 1;
                records.push(json!({"path":path,"variant":null,"status":"fail","stage":"setup","detail":error.to_string()}));
                continue;
            }
        };
        for execution in run_with_engine(&test, &manifest, &corpus, options.backend) {
            let result = execution.result;
            let variant = format!("{:?}", result.variant);
            let (status, reason, detail) = match result.status {
                ResultStatus::Passed => {
                    passed += 1;
                    ("pass", None, None)
                }
                ResultStatus::Failed(detail) => {
                    failed += 1;
                    ("fail", None, Some(detail))
                }
                ResultStatus::Skipped { reason, detail } => {
                    skipped += 1;
                    ("skip", Some(reason), Some(detail))
                }
            };
            let observed_failure = execution
                .observed_failure
                .map(|error| json!({"phase":format!("{:?}",error.phase),"type":error.error_type}));
            records.push(json!({"path":result.path,"variant":variant,"status":status,"reason":reason,"detail":detail,"diagnostic":execution.diagnostic,"observed_failure":observed_failure}));
        }
    }
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
        Ok(Some(options)) => match execute(options) {
            Ok(code) => ExitCode::from(code),
            Err(error) => {
                eprintln!("temiya-test262: {error}");
                ExitCode::from(1)
            }
        },
    }
}
