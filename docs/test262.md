# Test262 harness

The harness is pinned to commit
`d53ebc8b1931d9f1541e43d59650776e4b766248`. Run `scripts/test262` to fetch
that exact revision into `target/test262`; `TEST262_DIR` selects another cache.

`test262/manifest.json` records the same revision and exact-path skips. Every
skip has a stable reason identifier and a human explanation, so reports can be
reproduced and aggregated without interpreting free-form logs.

`temiya::test262::TestCase` parses the frontmatter subset used for includes,
features, flags, and negative phase/type expectations. It expands tests into
sloppy/strict variants in stable order, or a single module/raw/flag-constrained
variant. A strict Script receives the directive before parsing, ensuring parse
negative tests are exercised in the correct source variant.

## Engine runner

`temiya-test262` executes selected cases through the public `Engine`, including
compilation, module loading/linking and evaluation. Parsing alone never produces
a positive pass. The existing `Executor`/`run_test` API remains available;
`EngineExecutor` supplies a concrete adapter and `run_with_engine` additionally
applies metadata capability gates and retains original failure diagnostics.
The concrete adapter lives in the [CLI support module](../src/bin/support/test262_engine.rs)
and implements the existing public `Executor`/`run_test` contract. Git/filesystem
runner code is compiled into the CLI client rather than the engine library.

```sh
scripts/test262
cargo run --locked --bin temiya-test262 -- \
  --corpus target/test262 --pinned --backend auto --output /tmp/test262.json \
  test/built-ins/String/prototype/at/returns-item.js \
  test/language/statements/const/global-use-before-initialization-in-prior-statement.js
```

Paths are normalized, corpus-relative file paths. Multiple selected files run in
argument order; every required variant gets a fresh Engine and Realm. The default
backend is Auto; `ast` and `bytecode` explicitly select Script execution. Module
source runs through `Engine::load_module_with_resolver`,
`ModuleHandle::record().link()` and `Engine::evaluate_module`, independently of the
Script backend. Linking executes no module body, and `record().status()` lets the
runner reject incomplete module evaluation instead of treating it as success.

`Corpus::pinned` / `--pinned` reads tests, harness files and relative module fixtures
with `git show` at the fixed commit. Dirty and untracked working-tree files do not
enter a pinned result. The manifest revision must match the consumed pin.
`Corpus::directory` (the CLI without `--pinned`) supports authored local fixtures;
its JSON `corpus_revision` is null and makes no upstream qualification claim.
Absolute paths, parent traversal and filesystem links escaping the corpus are
rejected. Module fixtures resolve relative to their importing file; unsupported
bare specifiers, attributes and source-read failures remain runner failures.

For non-raw tests the adapter evaluates the unchanged corpus `assert.js`, then
`sta.js`, then additional includes in metadata order. The strict directive applies
to the test source, not the harness files. Raw source is unmodified and loads no
harness, including when metadata lists an include. A missing, unparsable or throwing
include is a `HarnessError`; it cannot satisfy a negative JavaScript expectation.
The CLI adapter extends the foundation parser through `parse_case`, retaining the
original JavaScript source bytes. Its inline lists accept plain names, single-quoted names and JSON-style double-quoted
strings; commas inside quoted include filenames are preserved. Malformed lists
fail rather than silently losing items. Unsupported broader YAML remains an
explicit frontmatter failure, not a successful execution.

Each JSON result records its path, variant, pass/fail/skip, reason, diagnostic and
the original observed failure phase/type, including matching negatives. Parse,
resolution/link and runtime failures must match both expected phase and error
constructor. Thrown objects are compared against captured constructor identities;
a forged `.name` string or a second error from a constructor getter does not
replace the original throw. Engine errors, unavailable host hooks, unsupported
Bytecode and incomplete module evaluation cannot match a negative expectation.
The runner drains synchronous jobs after successful evaluation; this is not an
implementation of asynchronous Test262 completion.

Exit codes are 0 for completed successful selections, 1 for test/setup failures,
2 for usage errors, and 3 when all selected variants were skipped. Counts separate
pass, fail and skip. File/frontmatter failures are reported as failed setup records
without inventing an executed variant. The checked-in exact-path skip manifest is
honored before execution. Async/agent and unknown runner flags get explicit skips;
conflicting strict/raw/module flags fail instead of silently choosing a variant.

The integrated runner and include/strict/raw handling cover #145/#146. `$262`
hooks (#147), `$DONE`/async completion (#148),
broader module fixture/negative audits (#149), process isolation/resource limits
(#150), feature baselines (#151) and regression selection (#152) remain separate
work. It does not currently impose per-test time or memory limits. A small
selection does not qualify the whole corpus or a complete language feature.

```sh
TEST262_DIR=target/test262 cargo test --locked --test test262_engine \
  pinned_ -- --ignored --nocapture
```

The first regression executes nine original files / thirteen variants through the actual CLI:
String `charAt` coercion and `at`, Array `pop`, a strict parse negative, a TDZ runtime
negative, a module resolution negative with its original fixture, two raw hashbang
cases and a noStrict legacy escape. It also
checks that authored working-tree harness files cannot override pinned Git input.
The normal integration tests exercise real Engine execution, loader order, raw
and strict distinctions, Realm separation, native/explicit throws, wrong phases,
harness failures, missing hooks, skips, malformed paths and CLI exit codes. CI
runs both original selections on stable Rust and Rust 1.85.

The second regression selects all 30 files previously marked failed and tracked
by #146 in the retained Array stack/String audits. All 60 variants are rerun for
each of AST and Auto with the original `propertyHelper.js` and other includes.
Historical audit records are preserved; they do not describe the current runner.
Failures from other incomplete APIs remain failures in the broader follow-up,
without replacement harness functions or success inferred from a loaded name.

The [qualification record](../benchmarks/test262-runner/README.md) retains the
original CLI reports, native/Clippy/Miri/CI logs, exact implementation inputs and
every performance experiment. Its verifier independently checks original
workload oracles, raw samples, statistical decisions and worker identities.
Inconclusive ARM64 comparisons remain inconclusive; the record does not turn
them into speed passes or qualify the whole Test262 corpus.
