# Temiya

Temiya is an experimental JavaScript engine written in Rust.

The project is named after Temiya in Otaru, the starting point of Hokkaido's
first railway. That railway carried coal from Horonai to the Port of Otaru:
an engine route powered by coal, remembered in the name of a new engine built
with Rust.

## Goal

Temiya aims to provide a Rust-native alternative to SpiderMonkey. The project
will implement the engine as original Rust code rather than translate Mozilla
source code. Compatibility targets will be defined by public standards and
observable embedding contracts.

The initial milestones are:

1. ECMAScript source text, lexer, and parser
2. Values, objects, execution contexts, and an interpreter
3. Tracing garbage collector with explicit rooting
4. ECMAScript conformance through Test262
5. Stable Rust embedding API and C ABI
6. Bytecode and tiered optimization after interpreter correctness

Temiya executes a subset of JavaScript through its Rust embedding API. It is
experimental: parsing a construct does not imply that it can be evaluated, and
the presence of a built-in name does not imply complete ECMAScript behavior.

```rust
use temiya::{Engine, runtime::Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine.compile(&realm, "function twice(x) { return x * 2; } twice(21);")?;
    let result = engine.evaluate(&script)?;
    assert!(matches!(result.value(), Value::Number(42.0)));
    Ok(())
}
```

The execution paths currently have different capabilities:

| Path | Entry point | Current scope and limits |
| --- | --- | --- |
| Parser | `parser::parse_script` / `parse_module` | Produces owned syntax; does not execute it. For example, a class expression parses but AST evaluation rejects it. |
| AST interpreter | `Engine::compile` then `Engine::evaluate` | Executes arithmetic, variables, loops, ordinary functions, and supported object operations. Select it explicitly with ScriptBackend::Ast; it also handles whole Scripts rejected by Auto before execution. Classes and dynamic import are among the missing runtime features; built-ins remain incomplete. |
| Bytecode interpreter | `BytecodeCompiler::compile` then `VirtualMachine::execute` | The default Engine uses Auto to select this whole-Script subset when supported: primitive operations, global/function/block bindings, TDZ/const, property references, calls/construct and simple-parameter functions, conditionals, while/for/do-while, break/continue and labels. Use `execute_in_realm` for persistent globals; standalone function-body execution requires a caller-prepared frame. Array literals, holes, iterable spread, data object literals, computed keys, object spread and prototype setters are supported; object methods and for-in/of remain unsupported. `Engine::builder().script_backend(ScriptBackend::Bytecode)` selects this backend and reports unsupported syntax during compilation; `Auto` falls back to AST before executing an unsupported Script and never retries a runtime error. |
| Experimental JIT | `BaselineJit::compile` with `--features jit` | Separate straight-line numeric backend on x86-64 Unix. For example, `40 + 2;` is supported, but `1 === 1;` is rejected. Other platforms, including ARM64, return `UnsupportedPlatform`. Enabling the feature does not make `Engine` use JIT execution. |

Run `cargo run --example backend_scope --all-features` to check these examples
through the public APIs, including the unsupported cases. See
[bytecode and JIT](docs/bytecode.md) for their explicit error contracts.

The 0.2 Engine integrates a shared tracing heap, allocation safepoints and
`Engine::collect_garbage`. Public handles retain live objects and closures;
unreachable cycles are reclaimed. Host-created objects use the Engine's
explicit allocation context; see [embedding](docs/embedding.md).
The traced runtime passes all33 fixed original-eleven omoikane modified-Boa
comparisons on each of two recorded x64 hosts. The local ARM64 follow-up has
32 faster decisions and1 within noise. Main regressions, machine variability,
source identities and complete raw evidence are preserved in the
[runtime qualification](docs/runtime-ownership.md). These results are limited
to the recorded workloads and conditions. Selected Test262 regressions and
full library Miri checks pass; whole-engine ECMAScript conformance remains
incomplete. Debugger and optimizing-tier contracts are development components.
Separate async, RegExp, component and pinned SpiderMonkey results are recorded
in the [benchmark coverage matrix](docs/benchmark-suite-coverage.md).
Broader application performance and SpiderMonkey superiority remain unverified.

An initial lexer is available as `temiya::lexer::tokenize`; its supported subset
and next steps are documented in [docs/lexer.md](docs/lexer.md).
Owned expression and statement parsers are available as
`temiya::parser::parse_expression`, `temiya::parser::parse_script`, and
`temiya::parser::parse_module`;
see [parser scope](docs/parser.md), [AST ownership](docs/ast.md), and
[source position conversion](docs/source.md).
ECMAScript values and fundamental abstract conversions are described in
[runtime values](docs/runtime.md).
Collector-independent handles, explicit roots, and tracing rules are described
in [garbage collector contracts](docs/gc.md).
The pinned conformance corpus and metadata rules are documented in the
[Test262 harness](docs/test262.md).
Run selected original cases with `cargo run --locked --bin temiya-test262 --
--corpus target/test262 --pinned TEST_PATH`; `$262` provides real child Realms,
Script evaluation and ArrayBuffer detachment. The runner reports pass, fail and
skip separately after actual Engine execution. It does not qualify the full
corpus. Async variants use the original `$DONE` harness, drive the job queue,
and have a finite worker deadline (`--timeout-ms`, default 10000).
Cross-engine result normalization and reproducer reduction are documented in
[differential testing](docs/differential-testing.md).
Lexer/parser fuzz targets, limits, corpus, and regression handling are described
in [fuzzing](docs/fuzzing.md).
Scheduled dependency, Miri, and sanitizer coverage is documented in
[security checks](docs/security-checks.md).
Performance inputs, environment capture, noise handling, and regression policy
are documented in [benchmarks](docs/benchmarks.md).
The selected ECMAScript edition, Annex B, proposals, and host boundaries are
tracked in the [specification support matrix](docs/spec-support.md).
The stable thread-confined Rust boundary is described in the
[embedding API](docs/embedding.md).
The portable instruction format and mandatory verifier are documented in
[bytecode](docs/bytecode.md).
Future optimizing-tier, deoptimization, OSR, GC, and debugger invariants are
specified in the [tiering design](docs/tiering.md).
Versioning, reproducible artifacts, SBOMs, checksums, and attestations are
described in the [release process](docs/releasing.md).

## Command-line execution

Run `cargo run --locked --bin temiya -- -e 'print(40 + 2);'`, or build the
release `temiya` binary to execute UTF-8 files and stdin. `--module` loads a
canonical file module graph; Script inputs share one global environment.
See [shell modes, output, errors and verification](docs/cli.md).

## Development

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

GitHub Actions runs formatting and Clippy checks on stable Rust, plus tests and
builds on stable Rust and the minimum supported version, Rust 1.85.0. CI runs
for pull requests, pushes to `main`, and manual dispatches.

Design decisions and compatibility scope will be documented before the
corresponding public APIs are stabilized.

## Sandboxed development

Temiya uses [Izanagi](https://github.com/ieee0824/izanagi) with Docker on macOS.
The development image uses Debian Trixie and pins its base to
`izanagi-vm-debian:0.2.5` so an Izanagi update cannot silently change the base
OS or libc. Its glibc agent is built from the matching Izanagi source revision.
The project is mounted read-write so sandboxed development agents can edit the
working tree. Cargo artifacts are written to the container's temporary
filesystem. No host user directories are mounted. Docker networking remains
available because Izanagi uses a forwarded port to communicate with its agent;
do not treat this setup as network-isolated.
The image includes pinned Codex CLI and Claude Code installations, plus GitHub
CLI. Authenticate them at runtime; credentials are deliberately not baked into
the image or mounted from the host.

`codex` runs with `--no-daemon` inside the sandbox because the shared app-server
cannot retain its PID-managed background process under the container process
model. The unwrapped entry point is available as `codex-daemon` for diagnostics.
Izanagi's environment policy preserves `CARGO_HOME` and `RUSTUP_HOME` from the
pinned development image for exec and interactive shells. Minimal `cargo` and
`rustc` wrappers restore their PATH until upstream image-PATH inheritance works.

Build the version-matched Debian base and local development image once. The
Izanagi source is expected at `../izanagi`; set `IZANAGI_SOURCE` to override it:

```sh
scripts/sandbox build-base
scripts/sandbox build
```

Run `izanagi init`, then apply the values from
`.izanagi/izanagi.toml.example` to the generated project-specific config. Keep
the active config outside this read-only shared workspace.

Start Izanagi in one terminal, then run checks from another:

```sh
scripts/sandbox up
scripts/sandbox test
scripts/sandbox logs
scripts/sandbox down
```

To use the host's existing Codex login in the running sandbox, copy only its
credential file after startup:

```sh
scripts/sandbox codex-auth
scripts/sandbox shell
codex login status
```

This grants processes in that sandbox the same Codex account authority as the
host. `CODEX_HOME` points to `.git/codex-sandbox`, so configuration, logs, and
session history persist across container recreation without becoming tracked
files. The credential is never baked into the image: `scripts/sandbox up` and
`scripts/sandbox down` remove `auth.json`, so run `scripts/sandbox codex-auth`
after every startup. An abrupt host shutdown can bypass the normal cleanup, but
the next `scripts/sandbox up` removes any stale credential before launch.

GitHub CLI can authenticate independently with its device flow. Configure Git
to reuse that login over HTTPS, avoiding the need to share an SSH key:

```sh
scripts/sandbox shell
gh auth login --web --git-protocol https
gh auth setup-git
```

The Izanagi configuration is stored outside the shared project directory under
`~/.izanagi/settings/`. This prevents sandboxed processes from reading or
modifying their own security policy.
