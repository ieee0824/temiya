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

## Development

```sh
cargo test --locked
cargo build --locked
```

Design decisions and compatibility scope will be documented before the
corresponding public APIs are stabilized.

## Sandboxed development

Temiya uses [Izanagi](https://github.com/ieee0824/izanagi) with Docker on macOS.
The development image pins its base to `izanagi-vm:0.2.4` so an Izanagi update
cannot silently change the base OS or libc.
The project is mounted read-only, and Cargo artifacts are written to the
container's temporary filesystem. No host credentials or user directories are
mounted. Docker networking remains available because Izanagi uses a forwarded
port to communicate with its agent; do not treat this setup as network-isolated.
The image includes pinned Codex CLI and Claude Code installations, plus GitHub
CLI. Authenticate them at runtime; credentials are deliberately not baked into
the image or mounted from the host.

Build the local development image once:

```sh
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
host. The credential is copied into the ephemeral container, never into the
image or repository, and disappears when the container stops. Run
`scripts/sandbox codex-auth` again after starting a new sandbox.

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
