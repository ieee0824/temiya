# Conformance and performance baselines

Run `scripts/bench`; it records the Git revision, `rustc` version, target
architecture, stable input identifiers, and all timing samples. The suite covers
Script parsing, parse-plus-interpreter macro execution, full GC, and Object
built-ins. `TEMIYA_BENCH_SAMPLES` and `TEMIYA_BENCH_ITERATIONS` are recorded
controls; reference runs use at least 15 samples and an otherwise idle runner.

Only compare runs from the same hardware, OS image, toolchain, profile, inputs,
and iteration count. Summaries use median and median absolute deviation (MAD).
A slowdown is a regression only when it exceeds both 10% and three times the
sum of baseline/current MAD-to-median ratios. Suspected regressions receive a
second run before filing; performance targets themselves are outside this policy.

`benchmarks/baseline.json` stores the reviewed reference environment, policy,
revision, samples, and results. Test262 conformance uses the separately pinned
revision and skip manifest in `test262/manifest.json`; changes to pass/skip/fail
counts must be reviewed alongside that manifest rather than mixed with timings.

## Cross-engine targets

The performance target is to exceed the Boa version embedded in
[omoikane](https://github.com/ieee0824/omoikane); exceeding SpiderMonkey is an
additional target. Neither has been demonstrated. The current internal suite
is insufficient evidence for either claim.

The initial application reference is omoikane revision
`007556adccb8fb4329236932ca8175f8e8d119d2`, including its vendored `engine/boa`,
vendored `engine/regress`, embedding configuration, and feature flags. Its
`tests/js_benchmark/shapes.js` fixture version 2 has eleven workloads and
independent result checks in `tests/engine_benchmark_results.rs`. An upstream
Boa release is not a substitute for that modified engine. Record a pinned
SpiderMonkey revision/build and actual execution mode when adding its results.

For a reviewable comparison:

- Run the same workload sources and iteration counts on the same machine in
  release builds. Record source hashes, engine revisions, compiler/build flags,
  architecture, OS, and whether JIT execution actually occurred.
- Check the expected result on every warmup and measured pass. Unsupported
  inputs, incorrect results, crashes, timeouts, and skipped tests cannot count
  as speedups. Isolate workloads in processes so one failure does not erase
  the remaining diagnostics.
- Report startup/parse costs separately from warmed execution. Identify whether
  a run uses the actual application embedding or a standalone engine context;
  they answer different questions. Preserve each workload's samples, median,
  and MAD; do not hide regressions behind an aggregate score.
- Use repeated runs to establish improvement beyond measurement noise. Keep
  correctness/conformance results and memory growth alongside timing results;
  leaking allocations or dropping observable work is not an optimization.

The eleven microbenchmarks provide an initial comparison, not proof that a
complete application or every JavaScript program runs faster. Broader workloads
and the outstanding compatibility work remain necessary before such claims.

### Array workload diagnostic

The array stack implementation enables omoikane's original array function body
at its original 500,000 iterations. Extract it from the pinned Git objects of an
omoikane checkout, retaining its MIT notice, then run the numeric result worker:

```sh
node scripts/extract-omoikane-array.mjs /path/to/omoikane \
    > /tmp/omoikane-array.js 2> /tmp/omoikane-array-provenance.json
cargo build --locked --release --example measure_number
timeout 180 target/release/examples/measure_number \
    /tmp/omoikane-array.js 124999750000 3
```

Node.js only extracts the function; it does not run the measured loop. The
worker separately records Realm creation, compilation, one discarded warmup,
and evaluation samples through Engine. It validates every result, including
warmup, against the independent sum `n * (n - 1) / 2`. An incorrect result or
engine error exits unsuccessfully without a valid timing report.

This replaces omoikane's JavaScript timing harness with a Rust host timer. It
does not run the full fixture, DOM embedding, or Boa/SpiderMonkey comparison.
Three samples are a diagnostic baseline, not a performance acceptance gate;
the complete comparison still requires the controls described above. The
current linear property storage is a known optimization target.
