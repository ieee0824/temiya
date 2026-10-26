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

Temiya is at the bootstrap stage and does not execute JavaScript yet.
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
