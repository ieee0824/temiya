# Differential testing

`temiya::differential` evaluates deterministic, terminating generated programs
through engine-independent adapters. `CommandEngine` uses only a process's
stdin/stdout/stderr and enforces a wall-clock timeout, avoiding dependencies on
another engine's internal API.

Results normalize line endings, trailing whitespace, and configured regexes for
timestamps, addresses, or other non-deterministic text before comparison.
Reviewed source patterns classify known differences by stable identifier.
Unexpected mismatches can be reduced line-by-line with `minimize_source` and
written with `save_reproducer`. Generator seeds, adapter commands, normalization
rules, and known-difference identifiers should accompany stored reproducers.

## Native Temiya and SpiderMonkey adapter

The native adapter uses the public Engine through `temiya-differential-worker`
and the pinned SpiderMonkey shell from [the target build](spidermonkey-target.md).
Build the Rust worker and supply the shell's verified build-info JSON:

```sh
cargo build --locked --release --no-default-features --bin temiya-differential-worker
node scripts/compare-js-engines.mjs \
  --temiya target/release/temiya-differential-worker \
  --spidermonkey path/to/js --spidermonkey-info path/to/build-info.json \
  --cases testing/differential/adapter-cases.json \
  --known testing/differential/known-differences.json \
  --mode interpreter --backend auto --timeout-ms 2000 --output target/differential
```

Use a new output directory for each run. `--temiya-info` optionally records and
checks the Temiya build's executable hash; the same path can be supplied through
`TEMIYA_DIFFERENTIAL_BUILD_INFO`. Interpreter mode disables SpiderMonkey Ion,
Baseline and blinterp and checks their actual settings. Default mode records
settings without claiming that a particular program exercised JIT compilation.

Each case compares the actual completion, ordered `record(...values)` effects,
`queueJob(label, callback)` execution, job exceptions and their combined event
order. Temiya uses Engine jobs; SpiderMonkey uses native Promise reactions and
`drainJobQueue`. Native unhandled rejections are tracked at the job checkpoint.
Compile errors, runtime errors and successful values remain distinct.

Numbers retain binary64 identity, including negative zero, with canonical NaN.
Strings retain UTF-16 code units, including lone surrogates. BigInts use decimal
values; Symbols and data objects preserve first-observed identities and aliases.
Plain objects, null-prototype objects and arrays retain ordered own descriptors,
holes, cycles and fresh snapshots after mutation. Native errors compare captured
exception types rather than unstable messages or stacks. The protocol validates
references, types, limits and the consistency of event projections before any
comparison. This prevents two malformed observations from becoming an equal pass.

Functions, accessors, unknown prototypes and unsupported observations are
ineligible; getters are not invoked to manufacture comparable values. Snapshot
limits are 32 active object levels, 1,024 identities and 4,096 properties per
root value. Exotic internal slots and general Proxy behavior are unqualified.
The controller applies finite process deadlines and an 8 MiB output limit;
timeout, signal, invalid UTF-8/JSON and worker faults remain infrastructure
failures. This is not a security sandbox for hostile source.

The output preserves case inputs, both raw stdout/stderr byte streams, exit and
signal records, conditions, executable/controller/worker hashes and normalized
observations. Known differences require the exact source hash and the exact
pair of observations, a reason, issue link and matching reproducer file. Changed
observations cannot inherit an old exception. Unexpected differences,
ineligible observations or infrastructure failures fail the run; capture identity
changes also fail. No generic text normalization suppresses native value changes.

`prepare-differential-test262.mjs` uses the real metadata-only CLI to prepare the
fixed selection in `testing/differential/test262-selection.json`. It retains
original harness includes and bodies with their hashes, adding only the strict
directive and separating newlines. Modules and general Test262 coverage are not
qualified by this selection.

Native run 40927712437 at source `484fe896b8a3b1c35b80e386384d64538096df96`
records 11 matching adapter cases and one exact known Symbol difference in each
mode, and five matching original Test262 variants in each mode. Separate cases
retain unsupported values, actual deadlines, the Promise/Symbol discoveries and
rejection of a modified known signature. Reproducers link the still-open #179
and #204; this adapter does not implement those language features. The empty
program supplies a matching reduction witness; a general AST reducer is not
provided by this command. These comparisons establish neither performance nor
whole-engine conformance.

The `differential-adapter.yml` reusable workflow runs this evidence through the
registered SpiderMonkey benchmark workflow. `adapter_only=true` selects native
comparison without requesting a separate timing run. Stable and MSRV CI also
run the actual Rust worker integration tests and protocol boundary tests.
