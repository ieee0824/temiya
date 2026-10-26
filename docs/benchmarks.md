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
