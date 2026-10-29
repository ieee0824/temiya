use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

#[path = "../src/bin/support/test262_engine.rs"]
mod engine;
use engine::{Corpus, EngineTestResult, parse_case, run_with_engine};
use temiya::{
    Engine,
    embedding::{ExceptionKind, ScriptBackend},
    runtime::{ModuleStatus, Value},
    test262::{Manifest, ResultStatus},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "temiya-test262-engine-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self(path);
        // Authored loader-order fixtures, never substituted for upstream files
        // in corpus qualification. Real pinned files are exercised below.
        fixture.write(
            "harness/assert.js",
            "var loaderOrder='a'; function assert(x){if(!x)throw new Error();}",
        );
        fixture.write("harness/sta.js", "loaderOrder+='s';");
        fixture.write("harness/extra.js", "loaderOrder+='i';");
        fixture
    }
    fn write(&self, name: &str, source: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    fn corpus(&self) -> Corpus {
        Corpus::directory(&self.0).unwrap()
    }
    fn run(&self, source: &str, backend: ScriptBackend) -> Vec<EngineTestResult> {
        run_with_engine(
            &parse_case("test/case.js", source).unwrap(),
            &manifest(),
            &self.corpus(),
            backend,
        )
    }
    fn cli(&self, args: &[&str]) -> (Output, serde_json::Value) {
        let output = Command::new(env!("CARGO_BIN_EXE_temiya-test262"))
            .arg("--corpus")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap();
        let report = serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
        (output, report)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn manifest() -> Manifest {
    Manifest::from_json(include_str!("../test262/manifest.json")).unwrap()
}
fn pass(results: &[EngineTestResult], expected: usize) {
    assert_eq!(results.len(), expected);
    assert!(
        results
            .iter()
            .all(|r| r.result.status == ResultStatus::Passed),
        "{results:?}"
    );
}
fn fail(results: &[EngineTestResult]) {
    assert!(!results.is_empty());
    assert!(
        results
            .iter()
            .all(|r| matches!(r.result.status, ResultStatus::Failed(_))),
        "{results:?}"
    );
}

#[test]
fn public_engine_links_without_running_bodies_and_rejects_foreign_handles() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    engine
        .register_global_function(&realm, "mark", move |context, _, _| {
            observed.set(observed.get() + 1);
            Ok(context.value(Value::Undefined))
        })
        .unwrap();
    let module = engine
        .load_module(&realm, "root", |_| {
            Ok("export var answer=42; mark();".into())
        })
        .unwrap();
    assert_eq!(module.record().status(), ModuleStatus::Unlinked);
    assert_eq!(
        Engine::default()
            .evaluate_module(&module)
            .unwrap_err()
            .kind(),
        ExceptionKind::Engine
    );
    module.record().link().unwrap();
    module.record().link().unwrap();
    assert_eq!(module.record().status(), ModuleStatus::Linked);
    assert_eq!(calls.get(), 0);
    engine.evaluate_module(&module).unwrap();
    assert_eq!(module.record().status(), ModuleStatus::Evaluated);
    assert_eq!(calls.get(), 1);
    module.record().link().unwrap();
    assert_eq!(calls.get(), 1);
    let bad = engine
        .load_module(&realm, "root", |name| {
            Ok(if name == "root" {
                "import {missing} from 'dep'; mark();"
            } else {
                "export var present=1;"
            }
            .into())
        })
        .unwrap();
    assert_eq!(
        bad.record().link().unwrap_err().kind(),
        temiya::runtime::RuntimeErrorKind::SyntaxError
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn public_runner_executes_bodies_and_matches_error_phase_and_constructor() {
    let fixture = Fixture::new();
    for backend in [
        ScriptBackend::Ast,
        ScriptBackend::Bytecode,
        ScriptBackend::Auto,
    ] {
        pass(
            &fixture.run("/*---\nflags: []\n---*/\nif(6*7!==42)throw 1;", backend),
            2,
        );
        pass(
            &fixture.run(
                "/*---\nnegative:\n  phase: runtime\n  type: TypeError\n---*/\nnull.x;",
                backend,
            ),
            2,
        );
        pass(&fixture.run("/*---\nnegative:\n  phase: runtime\n  type: SyntaxError\n---*/\nthrow new SyntaxError();",backend),2);
        pass(
            &fixture.run(
                "/*---\nnegative:\n  phase: parse\n  type: SyntaxError\n---*/\nlet = ;",
                backend,
            ),
            2,
        );
        fail(&fixture.run("/*---\nnegative:\n  phase: parse\n  type: SyntaxError\n---*/\nthrow new SyntaxError();",backend));
        fail(&fixture.run(
            "/*---\nnegative:\n  phase: runtime\n  type: TypeError\n---*/\n1+1;",
            backend,
        ));
        fail(&fixture.run("/*---\nflags: []\n---*/\nthrow 42;", backend));
        fail(&fixture.run("/*---\nflags: []\n---*/\nclass C {}", backend));
    }
}

#[test]
fn harness_failures_and_unavailable_hooks_never_satisfy_negative_tests() {
    let fixture = Fixture::new();
    fixture.write("harness/broken.js", "throw new TypeError();");
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        for include in ["broken.js", "missing.js"] {
            let source = format!(
                "/*---\nincludes: [{include}]\nnegative:\n  phase: runtime\n  type: TypeError\n---*/\nnull.x;"
            );
            let results = fixture.run(&source, backend);
            fail(&results);
            assert!(format!("{:?}", results[0]).contains("HarnessError"));
            assert!(
                results[0]
                    .diagnostic
                    .as_deref()
                    .is_some_and(|text| text.contains(include))
            );
        }
        for source in ["$DONE();", "print();"] {
            fail(&fixture.run(
                &format!(
                    "/*---\nnegative:\n  phase: runtime\n  type: ReferenceError\n---*/\n{source}"
                ),
                backend,
            ));
        }
        fail(&fixture.run(
            "/*---\nnegative:\n  phase: runtime\n  type: EngineError\n---*/\nclass C {}",
            backend,
        ));
        for source in ["$262.agent.start('');", "$262.AbstractModuleSource();"] {
            let results = fixture.run(
                &format!("/*---\nnegative:\n  phase: runtime\n  type: TypeError\n---*/\n{source}"),
                backend,
            );
            fail(&results);
            assert!(results.iter().all(|result| {
                result
                    .observed_failure
                    .as_ref()
                    .is_some_and(|error| error.error_type == "EngineError")
            }));
        }
        fail(&fixture.run("/*---\nincludes: [missing.js]\nnegative:\n  phase: runtime\n  type: HarnessError\n---*/\n1;",backend));
    }
}

#[test]
fn host_realms_evaluate_scripts_with_separate_globals_and_original_throws() {
    let fixture = Fixture::new();
    let source = r#"/*---
flags: []
---*/
assert($262.global === this);
var child = $262.createRealm();
assert(child.global !== this);
assert(child.global.Array !== Array);
assert(child.global.$262 === child);
assert(child.evalScript('var isolated=41; isolated+1;') === 42);
assert(child.global.isolated === 41);
assert(typeof isolated === 'undefined');
assert(child.evalScript('this;') === child.global);
assert(child.evalScript('var empty;') === undefined);
var grandchild = child.evalScript('$262.createRealm();');
assert(grandchild.global !== child.global);
assert(grandchild.evalScript('typeof isolated;') === 'undefined');
var sentinel = {};
child.global.sentinel = sentinel;
try { child.evalScript('throw sentinel;'); throw 1; }
catch (e) { assert(e === sentinel); }
try { child.evalScript('let = ;'); throw 1; }
catch (e) { assert(e.constructor === child.global.SyntaxError); }
try { child.evalScript('missingName;'); throw 1; }
catch (e) { assert(e.constructor === child.global.ReferenceError); }
child.evalScript('var nested=$262.createRealm(); $262.gc(); nested.evalScript("var survived=17;");');
assert(child.global.nested.global.survived === 17);
"#;
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        pass(&fixture.run(source, backend), 2);
        for source in [
            "$262.evalScript('missingName');",
            "function f(){$262.evalScript('missingName');} f();",
        ] {
            pass(&fixture.run(&format!("/*---\nnegative:\n  phase: runtime\n  type: ReferenceError\n---*/\n{source}"), backend), 2);
        }
    }
}

#[test]
fn host_detachment_updates_byte_length_and_rejects_invalid_buffers_and_keys() {
    let fixture = Fixture::new();
    let source = r#"/*---
flags: []
---*/
var buffer = new ArrayBuffer(8);
assert(buffer.byteLength === 8);
var child = $262.createRealm();
assert(child.detachArrayBuffer(buffer) === undefined);
assert(buffer.byteLength === 0);
assert($262.detachArrayBuffer(buffer) === undefined);
var retained = new ArrayBuffer(4);
try { $262.detachArrayBuffer(retained, 1); throw 1; }
catch (e) { assert(e.constructor === TypeError); }
assert(retained.byteLength === 4);
try { child.detachArrayBuffer({}); throw 1; }
catch (e) { assert(e.constructor === child.global.TypeError); }
"#;
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        pass(&fixture.run(source, backend), 2);
    }
}

#[test]
fn name_spoofing_and_error_getter_failures_do_not_replace_original_throws() {
    let fixture = Fixture::new();
    for source in [
        "throw {name:'TypeError'};",
        "throw {get constructor(){throw new TypeError();}};",
    ] {
        fail(&fixture.run(
            &format!("/*---\nnegative:\n  phase: runtime\n  type: TypeError\n---*/\n{source}"),
            ScriptBackend::Ast,
        ));
    }
    pass(
        &fixture.run(
            "/*---\nnegative:\n  phase: runtime\n  type: TypeError\n---*/\nthrow new TypeError();",
            ScriptBackend::Auto,
        ),
        2,
    );
}

#[test]
fn harness_order_raw_strict_variants_and_fresh_realms_are_observable() {
    let fixture = Fixture::new();
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        pass(
            &fixture.run(
                "/*---\nincludes: [extra.js]\n---*/\nassert(loaderOrder==='asi');",
                backend,
            ),
            2,
        );
        pass(&fixture.run("/*---\nflags: [raw]\nincludes: [missing.js]\n---*/\nif(typeof loaderOrder!=='undefined')throw 42;",backend),1);
        pass(&fixture.run("/*---\nflags: [onlyStrict]\n---*/\nfunction f(){return this;} if(f()!==undefined)throw 42;",backend),1);
        pass(&fixture.run("/*---\nflags: [noStrict]\n---*/\nfunction f(){return this;} if(f()!==this)throw 42;",backend),1);
        pass(
            &fixture.run(
                "/*---\nflags: []\n---*/\nif(typeof leaked!=='undefined')throw 42; var leaked=1;",
                backend,
            ),
            2,
        );
    }
}

#[test]
fn module_parse_link_and_runtime_failures_are_separate() {
    let fixture = Fixture::new();
    fixture.write("test/dep_FIXTURE.js", "export var answer=42;");
    fixture.write("test/bad_FIXTURE.js", "export const = ;");
    pass(&fixture.run("/*---\nflags: [module]\nnegative:\n  phase: resolution\n  type: SyntaxError\n---*/\nimport './bad_FIXTURE.js';",ScriptBackend::Auto),1);
    fail(&fixture.run("/*---\nflags: [module]\nnegative:\n  phase: parse\n  type: SyntaxError\n---*/\nimport './bad_FIXTURE.js';",ScriptBackend::Auto));
    pass(&fixture.run("/*---\nflags: [module]\n---*/\nimport {answer} from './dep_FIXTURE.js'; if(answer!==42)throw 1;",ScriptBackend::Auto),1);
    pass(&fixture.run("/*---\nflags: [module]\nnegative:\n  phase: resolution\n  type: SyntaxError\n---*/\nimport {missing} from './dep_FIXTURE.js'; throw 1;",ScriptBackend::Auto),1);
    fail(&fixture.run("/*---\nflags: [module]\nnegative:\n  phase: runtime\n  type: SyntaxError\n---*/\nimport {missing} from './dep_FIXTURE.js';",ScriptBackend::Auto));
    pass(&fixture.run("/*---\nflags: [module]\nnegative:\n  phase: runtime\n  type: TypeError\n---*/\nthrow new TypeError();",ScriptBackend::Auto),1);
    pass(&fixture.run("/*---\nflags: [module]\nnegative:\n  phase: parse\n  type: SyntaxError\n---*/\nexport const = ;",ScriptBackend::Auto),1);
    fail(&fixture.run("/*---\nflags: [module]\nnegative:\n  phase: resolution\n  type: TypeError\n---*/\nimport './missing.js';",ScriptBackend::Auto));
}

#[test]
fn cli_runs_real_cases_and_keeps_setup_failures_and_skips_out_of_pass_counts() {
    let fixture = Fixture::new();
    fixture.write("test/pass.js", "/*---\nflags: []\n---*/\nassert(6*7===42);");
    fixture.write(
        "test/negative.js",
        "/*---\nnegative:\n  phase: runtime\n  type: TypeError\n---*/\nnull.x;",
    );
    fixture.write(
        "test/parse.js",
        "/*---\nnegative:\n  phase: parse\n  type: SyntaxError\n---*/\nlet = ;",
    );
    fixture.write("test/async.js", "/*---\nflags: [async]\n---*/\nthrow 42;");
    fixture.write("test/bad.js", "/*---\nflags: [raw]\n---*/\nthrow 42;");
    let (output, report) = fixture.cli(&[
        "test/pass.js",
        "test/negative.js",
        "test/parse.js",
        "test/async.js",
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(report["corpus_revision"].is_null());
    assert_eq!(
        report["counts"],
        serde_json::json!({"pass":6,"fail":0,"skip":2})
    );
    let (output, report) = fixture.cli(&["test/async.js"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(report["counts"]["pass"], 0);
    let (output, report) = fixture.cli(&["test/bad.js", "test/missing.js"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["counts"]["fail"], 2);
    assert_eq!(report["results"][1]["stage"], "setup");
    let (output, _) = fixture.cli(&["--backend", "invalid", "test/pass.js"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn corpus_and_include_paths_cannot_escape_their_roots() {
    let fixture = Fixture::new();
    assert!(fixture.corpus().revision().is_none());
    for path in [
        "../Cargo.toml",
        "/etc/passwd",
        "test/../case.js",
        "test\\case.js",
    ] {
        assert!(fixture.corpus().read(path).is_err());
    }
    fail(&fixture.run("/*---\nincludes: [../test/case.js]\nnegative:\n  phase: runtime\n  type: TypeError\n---*/\nnull.x;",ScriptBackend::Ast));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/passwd", fixture.0.join("escape.js")).unwrap();
        assert!(fixture.corpus().read("escape.js").is_err());
    }
}

#[test]
fn unknown_protocols_and_conflicting_flags_do_not_become_passes() {
    let fixture = Fixture::new();
    for flags in ["async", "CanBlockIsTrue", "'async'", "futureProtocol"] {
        let result = fixture.run(
            &format!("/*---\nflags: [{flags}]\n---*/\n1;"),
            ScriptBackend::Auto,
        );
        assert!(
            result
                .iter()
                .all(|r| matches!(r.result.status, ResultStatus::Skipped { .. }))
        );
    }
    for flags in [
        "onlyStrict, noStrict",
        "onlyStrict, raw",
        "module, raw",
        "module, noStrict",
    ] {
        fail(&fixture.run(
            &format!("/*---\nflags: [{flags}]\n---*/\n1;"),
            ScriptBackend::Auto,
        ));
    }
    let mut policy = manifest();
    policy.skips.insert(
        "test/case.js".into(),
        temiya::test262::SkipEntry {
            path: "test/case.js".into(),
            reason: "explicit-test-policy".into(),
            detail: "authored skip regression".into(),
        },
    );
    let case = parse_case("test/case.js", "/*---\nflags: []\n---*/\nthrow 42;").unwrap();
    let result = run_with_engine(&case, &policy, &fixture.corpus(), ScriptBackend::Auto);
    assert_eq!(result.len(), 2);
    assert!(
        result
            .iter()
            .all(|r| matches!(r.result.status, ResultStatus::Skipped { .. })
                && r.observed_failure.is_none())
    );
}

#[test]
fn quoted_flow_lists_preserve_flags_and_include_filenames() {
    let fixture = Fixture::new();
    fixture.write("harness/extra,comma.js", "loaderOrder+='c';");
    pass(&fixture.run("/*---\nflags: [\"onlyStrict\"]\nincludes: ['extra,comma.js']\n---*/\nfunction f(){return this;} assert(f()===undefined && loaderOrder==='asc');",ScriptBackend::Auto),1);
    pass(&fixture.run("/*---\nflags: ['raw']\nincludes: [\"missing.js\"]\n---*/\nif(typeof loaderOrder!=='undefined')throw 42;",ScriptBackend::Auto),1);
    pass(&fixture.run("/*---\nflags: ['noStrict', generated,]\n---*/\nfunction f(){return this;} if(f()!==this)throw 42;",ScriptBackend::Auto),1);
    for flags in ["[\"raw]", "[raw,,noStrict]", "['raw' junk]", "[,raw]"] {
        assert!(
            parse_case("bad.js", format!("/*---\nflags: {flags}\n---*/\n1;")).is_err(),
            "{flags}"
        );
    }
}

#[test]
#[ignore = "requires fixed Test262 corpus; TEST262_DIR selects the Git object store"]
fn pinned_original_cases_run_through_actual_cli_and_immutable_sources() {
    let upstream = std::env::var("TEST262_DIR").expect("set TEST262_DIR");
    let object_dir = Command::new("git")
        .args([
            "-C",
            &upstream,
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ])
        .output()
        .unwrap();
    assert!(object_dir.status.success());
    let fixture = Fixture::new();
    assert!(
        Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(&fixture.0)
            .status()
            .unwrap()
            .success()
    );
    fs::write(
        fixture.0.join(".git/objects/info/alternates"),
        &object_dir.stdout,
    )
    .unwrap();
    // The local authored loader files conflict with upstream; pinned mode must
    // read the original Git objects rather than those files.
    let pinned = Corpus::pinned(&fixture.0).unwrap();
    assert_ne!(
        pinned.read("harness/assert.js").unwrap(),
        fixture.corpus().read("harness/assert.js").unwrap()
    );
    let cases = [
        "test/built-ins/String/prototype/charAt/pos-coerce-err.js",
        "test/built-ins/String/prototype/at/returns-item.js",
        "test/built-ins/Array/prototype/pop/S15.4.4.6_A1.1_T1.js",
        "test/language/statements/let/syntax/identifier-let-allowed-as-lefthandside-expression-strict.js",
        "test/language/statements/const/global-use-before-initialization-in-prior-statement.js",
        "test/language/module-code/instn-star-err-not-found.js",
        "test/language/comments/hashbang/use-strict.js",
        "test/language/comments/hashbang/line-terminator-carriage-return.js",
        "test/language/literals/string/legacy-non-octal-escape-sequence-8-non-strict.js",
    ];
    for path in cases {
        assert!(pinned.read(path).is_ok(), "{path}");
    }
    let mut args = vec!["--pinned"];
    args.extend(cases);
    let (output, report) = fixture.cli(&args);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{report}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(report["corpus_revision"], temiya::test262::TEST262_REVISION);
    assert_eq!(
        report["counts"],
        serde_json::json!({"pass":13,"fail":0,"skip":0})
    );
    let observed: Vec<_> = report["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["observed_failure"]["phase"].as_str())
        .collect();
    assert_eq!(observed, ["Parse", "Runtime", "Runtime", "Resolution"]);
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    assert!(Path::new(&upstream).exists());
}

#[test]
#[ignore = "requires fixed Test262 corpus; rechecks all prior #146 dependency cases"]
fn pinned_harness_dependency_followup() {
    let upstream = std::env::var("TEST262_DIR").expect("set TEST262_DIR");
    let selected: Vec<String> = [
        include_str!("../test262/array-stack-audit.json"),
        include_str!("../test262/string-methods-audit.json"),
    ]
    .into_iter()
    .flat_map(|source| {
        let audit: serde_json::Value = serde_json::from_str(source).unwrap();
        audit["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r["status"] == "fail"
                    && r["tracking"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|id| id == 146))
            })
            .map(|r| r["path"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    })
    .collect();
    assert_eq!(
        selected.len(),
        30,
        "do not silently shrink the prior #146 audit"
    );
    for backend in ["ast", "auto"] {
        let output = Command::new(env!("CARGO_BIN_EXE_temiya-test262"))
            .args(["--corpus", &upstream, "--pinned", "--backend", backend])
            .args(&selected)
            .output()
            .unwrap();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{report}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            report["counts"],
            serde_json::json!({"pass":60,"fail":0,"skip":0})
        );
        assert_eq!(report["corpus_revision"], temiya::test262::TEST262_REVISION);
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    }
}

#[test]
#[ignore = "requires fixed Test262 corpus; original host-hook regressions"]
fn pinned_host_hooks_execute_original_cases() {
    let upstream = std::env::var("TEST262_DIR").expect("set TEST262_DIR");
    let mut selected =
        vec!["test/built-ins/Array/length/define-own-prop-length-overflow-realm.js".to_owned()];
    // Fixed positive regression selection; broader audits retain unsupported
    // integer TypedArray/SharedArrayBuffer, class and direct-eval failures.
    selected.extend(
        [
            "detached-buffer.js",
            "invoked-as-accessor.js",
            "invoked-as-func.js",
            "length.js",
            "name.js",
            "prop-desc.js",
            "return-bytelength.js",
            "this-is-not-object.js",
        ]
        .map(|name| format!("test/built-ins/ArrayBuffer/prototype/byteLength/{name}")),
    );
    selected.extend(
        [
            "script-decl-func-dups.js",
            "script-decl-func-err-non-configurable.js",
            "script-decl-func-err-non-extensible.js",
            "script-decl-func.js",
            "script-decl-lex-restricted-global.js",
            "script-decl-lex-var.js",
            "script-decl-var-err.js",
            "script-decl-var.js",
        ]
        .map(|name| format!("test/language/global-code/{name}")),
    );
    assert_eq!(selected.len(), 17);
    for backend in ["ast", "auto"] {
        let output = Command::new(env!("CARGO_BIN_EXE_temiya-test262"))
            .args(["--corpus", &upstream, "--pinned", "--backend", backend])
            .args(&selected)
            .output()
            .unwrap();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(output.status.code(), Some(0), "{report}");
        assert_eq!(
            report["counts"],
            serde_json::json!({"pass":34,"fail":0,"skip":0})
        );
        assert_eq!(report["corpus_revision"], temiya::test262::TEST262_REVISION);
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    }
}
