use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "temiya-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn write(&self, name: &str, source: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    fn run(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_temiya"))
            .current_dir(&self.0)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn expect(output: Output, code: i32, stdout: &str) -> String {
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(code), "{stderr}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        stdout,
        "{stderr}"
    );
    stderr
}

#[test]
fn script_inputs_share_globals_in_argument_order_and_drain_each_checkpoint() {
    let dir = Directory::new();
    dir.write("middle.js", "shared += 2; print(shared); async function f(){return 9;} f().then(function(x){shared=x;});");
    expect(
        dir.run(
            &[
                "-e",
                "let shared = 40;",
                "middle.js",
                "-e",
                "print(shared);",
            ],
            "",
        ),
        0,
        "42\n9\n",
    );
    expect(
        dir.run(&["-e", "var a=1;", "-e", "var a=2; print(a);"], ""),
        0,
        "2\n",
    );
    let error = expect(
        dir.run(
            &[
                "-e",
                "let a=1;",
                "-e",
                "let a=2;",
                "-e",
                "print('unreachable');",
            ],
            "",
        ),
        1,
        "",
    );
    assert!(error.contains("<eval:2>"), "{error}");
}

#[test]
fn unicode_stdin_output_conversion_and_separate_streams() {
    let dir = Directory::new();
    let error = expect(
        dir.run(
            &[],
            "var 答え = 42; print('小樽😀', 答え, null, undefined); console.error('診断', 7);",
        ),
        0,
        "小樽😀 42 null undefined\n",
    );
    assert_eq!(error, "診断 7\n");
    expect(
        dir.run(&["-", "-e", "print(value);"], "var value='stdin'; print();"),
        0,
        "\nstdin\n",
    );
    expect(
        dir.run(
            &[
                "-e",
                "var o={toString:function(){print('convert'); return 'object';}}; print(o, -0);",
            ],
            "",
        ),
        0,
        "convert\nobject 0\n",
    );
    expect(
        dir.run(
            &["-e", "String=function(){return 'wrong';}; print(42);"],
            "",
        ),
        0,
        "42\n",
    );
    expect(dir.run(&["-e", "var reason={}; var o={toString:function(){throw reason;}}; try { print(o); } catch(e) { print(e===reason); }"], ""), 0, "true\n");
    expect(dir.run(&["-e", r"print('\uD800');"], ""), 0, "�\n");
}

#[test]
fn errors_stop_execution_and_use_stderr_and_nonzero_status() {
    let dir = Directory::new();
    let error = expect(
        dir.run(&["-e", "let = ;", "-e", "print('unreachable');"], ""),
        1,
        "",
    );
    assert!(error.contains("compile failed"), "{error}");
    let error = expect(
        dir.run(
            &[
                "-e",
                "print('before'); throw '失敗';",
                "-e",
                "print('unreachable');",
            ],
            "",
        ),
        1,
        "before\n",
    );
    assert!(error.contains("失敗"), "{error}");
    let error = expect(
        dir.run(&["-e", "async function f(){throw 17;} f();"], ""),
        1,
        "",
    );
    assert!(error.contains("17"), "{error}");
    assert!(expect(dir.run(&["missing.js"], ""), 1, "").contains("missing.js"));
    fs::write(dir.0.join("bad-utf8.js"), [0xff, 0xfe]).unwrap();
    assert!(expect(dir.run(&["bad-utf8.js"], ""), 1, "").contains("bad-utf8.js"));
}

#[test]
fn command_line_errors_help_and_dash_filenames() {
    let dir = Directory::new();
    for args in [&["-e"][..], &["--unknown"], &["-", "-"]] {
        assert!(!expect(dir.run(args, ""), 2, "").is_empty());
    }
    let help = dir.run(&["--help"], "");
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout).unwrap().contains("--module"));
    expect(
        dir.run(&["--version"], ""),
        0,
        concat!("temiya ", env!("CARGO_PKG_VERSION"), "\n"),
    );
    dir.write("--script", "print('file');");
    expect(dir.run(&["--", "--script"], ""), 0, "file\n");
    expect(
        dir.run(
            &["--module", "--script", "-e", "print(this === globalThis);"],
            "",
        ),
        0,
        "true\n",
    );
}

#[test]
fn modules_resolve_nested_imports_aliases_and_distinct_relative_names() {
    let dir = Directory::new();
    dir.write("entry.js", "import {a} from './a/entry.js'; import {b} from './b/entry.js'; import {n} from './shared.js'; import {n as again} from './a/../shared.js'; print(a,b,n,again); var privateName=99;");
    dir.write(
        "a/entry.js",
        "import {value} from './dep.js'; export const a=value;",
    );
    dir.write(
        "b/entry.js",
        "import {value} from './dep.js'; export const b=value;",
    );
    dir.write("a/dep.js", "export const value=20;");
    dir.write("b/dep.js", "export const value=22;");
    dir.write("shared.js", "print('once'); export const n=42;");
    expect(
        dir.run(
            &[
                "--module",
                "entry.js",
                "shared.js",
                "-e",
                "print(typeof privateName);",
            ],
            "",
        ),
        0,
        "once\n20 22 42 42\nundefined\n",
    );
    expect(
        dir.run(&["--module"], "import {n} from './shared.js'; print(n);"),
        0,
        "once\n42\n",
    );
    dir.write(
        "cycle-a.js",
        "import './cycle-b.js'; export const a=1; print('a');",
    );
    dir.write(
        "cycle-b.js",
        "import './cycle-a.js'; export const b=2; print('b');",
    );
    expect(dir.run(&["--module", "cycle-a.js"], ""), 0, "b\na\n");
    dir.write("小樽.js", "export const 答え=42;");
    expect(
        dir.run(
            &[
                "--module",
                "-e",
                "import {答え} from './小樽.js'; print(答え, this === undefined);",
            ],
            "",
        ),
        0,
        "42 true\n",
    );
}

#[test]
#[cfg(unix)]
fn module_symlink_aliases_share_one_evaluation() {
    let dir = Directory::new();
    dir.write("source.js", "print('once'); export const n=42;");
    std::os::unix::fs::symlink(dir.0.join("source.js"), dir.0.join("alias.js")).unwrap();
    expect(
        dir.run(&["--module", "source.js", "alias.js"], ""),
        0,
        "once\n",
    );
}

#[test]
fn module_parse_link_resolution_and_evaluation_failures_are_real_failures() {
    let dir = Directory::new();
    dir.write("bad.js", "export const = ;");
    assert!(expect(dir.run(&["--module", "bad.js"], ""), 1, "").contains("bad.js"));
    for source in [
        "import './absent.js';",
        "import 'bare';",
        "import './absent.js' with { type: 'json' };",
        "import '\\uD800';",
        "export {missing};",
        "throw 'module failure';",
    ] {
        let error = expect(dir.run(&["--module", "-e", source], ""), 1, "");
        assert!(!error.is_empty(), "{source}");
    }
    dir.write("first.js", "print('must not execute');");
    expect(dir.run(&["--module", "first.js", "bad.js"], ""), 1, "");
    expect(
        dir.run(
            &[
                "--module",
                "-e",
                "var moduleOnly=1;",
                "-e",
                "print(typeof moduleOnly);",
            ],
            "",
        ),
        0,
        "undefined\n",
    );
}

#[test]
#[ignore = "requires the pinned Test262 checkout; executed explicitly in CI"]
fn pinned_test262_through_actual_shell() {
    use temiya::test262::{TEST262_REVISION, TestCase, TestVariant};
    let corpus = PathBuf::from(std::env::var_os("TEST262_DIR").expect("TEST262_DIR"));
    let read = |relative: &str| {
        let output = Command::new("git")
            .args(["-c", &format!("safe.directory={}", corpus.display()), "-C"])
            .arg(&corpus)
            .args(["show", &format!("{TEST262_REVISION}:{relative}")])
            .output()
            .unwrap();
        assert!(output.status.success(), "missing pinned source {relative}");
        String::from_utf8(output.stdout).unwrap()
    };
    let dir = Directory::new();
    dir.write(
        "harness.js",
        &format!("{}\n{}", read("harness/sta.js"), read("harness/assert.js")),
    );
    let mut passed = 0;
    for relative in [
        "test/language/statements/throw/S12.13_A1.js",
        "test/language/white-space/after-regular-expression-literal-ideographic-space.js",
        "test/language/module-code/export-default-function-declaration-binding.js",
        "test/language/module-code/early-dup-lex.js",
    ] {
        let case = TestCase::parse(relative, read(relative)).unwrap();
        assert!(case.metadata.includes.is_empty());
        for variant in case.variants() {
            dir.write("case.js", &case.source_for(variant));
            let args = if variant == TestVariant::Module {
                vec!["--module", "case.js"]
            } else {
                vec!["harness.js", "case.js"]
            };
            let output = dir.run(&args, "");
            let stderr = String::from_utf8_lossy(&output.stderr);
            if case.metadata.negative.is_some() {
                assert_eq!(output.status.code(), Some(1), "{relative}: {stderr}");
                assert!(stderr.contains("SyntaxError"), "{relative}: {stderr}");
            } else {
                assert!(output.status.success(), "{relative} {variant:?}: {stderr}");
                assert!(stderr.is_empty());
            }
            assert!(output.stdout.is_empty());
            passed += 1;
            println!("PASS {relative} {variant:?}");
        }
    }
    assert_eq!(passed, 6);
    println!("Test262 {TEST262_REVISION}: {passed} passed; 0 failed; 0 skipped");
}
