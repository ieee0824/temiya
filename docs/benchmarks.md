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
