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
rejected. Module fixtures resolve relative to their importing file; `.` and `..`
are canonicalized inside the corpus root, and canonical identities share one
module record. Unsupported bare specifiers, attributes and source-read failures
remain runner failures. The UTF-8 file adapter rejects unpaired UTF-16 module
specifiers without substituting U+FFFD or loading a different file. Valid
replacement-character and supplementary-code-point filenames remain supported.

For non-raw tests the adapter evaluates the unchanged corpus `assert.js`, then
`sta.js`, then (for async tests) the original `doneprintHandle.js`, then
additional includes in metadata order. The strict directive applies
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
The runner drains queued jobs and checks async completion after the queue reaches
quiescence. Rejection notifications run after ordinary queued reactions, so a
handler attached by another job can suppress an unhandled rejection.

Exit codes are 0 for completed successful selections, 1 for test/setup failures,
2 for usage errors, and 3 when all selected variants were skipped. Counts separate
pass, fail and skip. File/frontmatter failures are reported as failed setup records
without inventing an executed variant. The checked-in exact-path skip manifest is
honored before execution. Agent and unknown runner flags get explicit skips;
conflicting strict/raw/module flags fail instead of silently choosing a variant.

The integrated runner and include/strict/raw handling cover #145/#146. The
`$262` realm, evalScript and detach hooks are connected by #147.
Async completion and a hard async worker deadline are connected by #148.
Relative module fixtures and resolution-negative audits are connected by #149.
General process isolation/resource limits (#150), feature baselines (#151) and regression selection (#152) remain
separate work. Synchronous tests have no execution deadline; neither mode imposes
a memory limit. A small selection does not qualify the whole corpus or a complete
language feature.

The module selection in `tests/fixtures/test262-module/selection.json` reads
nineteen original files and their original dependencies at the fixed pin. Each
of AST and Auto executes nineteen Module variants, covering cyclic import and
re-export graphs, missing/renamed exports, resolution SyntaxErrors and basic
top-level await. Module bodies use the ModuleGraph AST path for both selections;
these are not Bytecode module qualifications. Named indirect exports are resolved
while linking, so an invalid re-export fails before any module body executes.
Link failures retain their phase and cannot be replaced by a later body throw.

The broader instantiation and top-level-await audits retain failures from missing
constructors, class evaluation, function initialization and pending await. The
focused cases do not qualify those APIs or the full async module lifecycle
(#179/#195/#196). Harness files are unchanged; no replacement Promise or global
function is inserted to turn an incomplete API into a pass.

Async variants run in separate CLI child processes. `--timeout-ms` accepts
1..120000 milliseconds (default 10000) per variant, including worker startup,
harness loading and execution. The parent kills and reaps a nonreturning worker;
a worker watchdog also terminates an infinite body or job. Missing completion
waits until the deadline and fails as `AsyncIncomplete`; a nonreturning worker
fails as `AsyncTimeout`. Neither counts as pass or skip. Metadata/manifest policy
gates run before spawning a worker. The source/manifest snapshot is sent over
stdin; pinned workers verify source against the immutable corpus again. Input
is bounded at 32 MiB, report output at 1 MiB and stderr at 64 KiB. Output overflow
fails as a harness error, while readers keep draining to avoid pipe deadlock.

The original harness calls host `print` with `Test262:AsyncTestComplete` or
`Test262:AsyncTestFailure:`. The runner records `async_outcome` as `complete`,
`failure`, `rejected`, `duplicate-completion`, `incomplete`, `timeout`, or `worker-error`;
expired records also have `timed_out: true`. A failure, duplicate notification,
unhandled rejection or missing notification cannot satisfy a runtime negative.
Parse/resolution negatives that finish before execution require no `$DONE`.
Notifications from setup cannot complete a test, and completion is frozen before
exception-inspection getters execute. Raw tests load no harness and may use
host print directly. The standalone synchronous `run_with_engine` adapter still
skips async tests; hard deadlines are supplied by the actual CLI supervisor.

```sh
TEST262_DIR=target/test262 cargo test --locked --test test262_async \
  pinned_ -- --ignored --nocapture
```

This regression verifies the original harness bytes and executes the original
async-function return/throw cases: four variants in each of AST and Auto.
Cases requiring the missing global `Promise.resolve` remain failures (#179);
these focused passes do not qualify the Promise constructor or the whole async
language feature. Authored CLI regressions additionally cover FIFO across child
Realms, rejection handling, duplicates, raw/setup boundaries and infinite jobs.

The [async qualification record](../benchmarks/test262-async-signals/README.md)
retains the original CLI executions, GC validation and new native comparisons.
Both recorded x64 cohorts are faster than actual modified Boa on all eleven
inputs, with all before/after decisions within the noise gate. ARM64 retains
slower and inconclusive outcomes. The initial candidate's reproducible losses
and modeled counters remain separately preserved; no whole-corpus or universal
speed claim follows from these focused measurements.

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

The host environment installs `$262.global`, `createRealm`, `evalScript`,
`detachArrayBuffer` and a real Engine-backed `gc` in the initial and child
Realms, including raw variants. Child Realms have separate globals/intrinsics
and recursively receive the host API, without inheriting the parent test harness.
The global `$262` binding is writable and configurable, and is not enumerable.
`evalScript` evaluates a real Script record in the function's original Realm,
returns its completion value and preserves thrown value identity and the error
Realm. Empty completion returns undefined. Registry borrows end before JavaScript
reentry or collection. Native callbacks hold weak owner references; the owner
roots every child Realm for that test variant, then releases them together.

Detachment uses the real ArrayBuffer slots and invalidates existing runtime
TypedArray/DataView reads and writes. The ordinary-buffer `byteLength` getter is
connected so original JS tests observe zero after detach. Non-buffer receivers
and nonmatching detach keys throw TypeError in the hook's Realm. Complete integer
TypedArray globals (#245), broader DataView APIs (#249), SharedArrayBuffer and
resizable/detach-key facilities remain separate work. An unsupported `agent` or
`AbstractModuleSource` access is an Engine failure, preventing accidental success
for a language TypeError negative. Optional IsHTMLDDA is not provided.

The current Script parser accepts UTF-8 source. An evalScript input containing
unpaired UTF-16 code units is reported as an Engine failure rather than silently
replacing source characters or satisfying a language SyntaxError negative. Full
UTF-16 Script input remains unqualified and is tracked by #527. The fixed host regression selects 17
original files / 34 variants in each of AST and Auto, including the previously
tracked Array length cross-Realm case. Broader byteLength and global-code audits
retain missing typed globals, class declarations and direct-eval failures; they
do not qualify those other features.

The [host-hook qualification record](../benchmarks/test262-host-hooks/README.md)
retains all native comparisons, one planned ARM replay, identical-executable
controls and matched full-input instruction diagnostics. Both recorded x64
cohorts pass all 33 actual-Boa comparisons. ARM64 has inconclusive cases and
mixed before/after outcomes; the record does not claim universal superiority or
the absence of a baseline regression.
