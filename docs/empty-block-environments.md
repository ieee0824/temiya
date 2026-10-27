# Empty AST block environments

The AST interpreter skips creating a declarative environment for a block that
has no direct let/const, function, class, or labelled statement. Actual block
bindings still get their own environment. Var declarations still target the
variable environment. This removes empty records and extra name-lookup hops
without specializing any workload or changing the public execution backend.

Public Engine regressions cover completion values, continue/break, var sharing,
block-let closure capture, TDZ, block functions, and observable with/Proxy lookup
and closure capture. The existing classic `for(let)` closure bug reproduces on
unchanged main and remains tracked in #173; this optimization does not fix it.

## ARM64 before/after

[Raw samples and provenance](../benchmarks/empty-block-arm64-beforeafter.json)
compare runtime `05731af024f2df61a2157cbca628d3e8caefeba5` against main
`c216836ebd9d7788d910c98793fb8281614ff393`, using separate release build targets
with Rust 1.99.1 on Linux ARM64 (four logical CPUs; CPU model unavailable).
All local builds completed before timing. Both workers receive the unchanged
original eleven omoikane inputs pinned at
`007556adccb8fb4329236932ca8175f8e8d119d2`, with 15 samples plus checked warmup,
three alternating pairs, and the same 512 MiB / 180 second watchdogs. Engine
startup and shared function setup are outside invocation timings.

All 66 worker runs completed successfully, validating 1056 results. Independent
recomputation checked the input hashes/oracles, binary hashes, raw medians/MAD,
ratios, thresholds, and classifications. Of 33 comparisons, two were faster
beyond the fixed noise threshold and 31 were within it; none were slower beyond
it. The two faster pairs were string-concat pair 1 and primitive-string-property
pair 0. Other repetitions of those workloads remained within threshold, so this
is not evidence of a consistent speedup for either workload or all eleven.
The all-eleven speed gate is false. The completed Boa and hosted comparisons are recorded below.

| Workload | After/before median ratios (three pairs) |
| --- | --- |
| arith | 0.906, 0.979, 1.006 |
| prop-mono | 0.911, 1.063, 0.973 |
| prop-mega | 0.945, 0.971, 0.915 |
| call | 1.016, 0.998, 0.973 |
| closure-alloc | 0.940, 0.977, 0.936 |
| object-alloc | 0.922, 0.956, 0.975 |
| string-concat | 0.912, 0.860, 0.870 |
| array | 0.929, 0.905, 0.914 |
| primitive-string-property | 0.897, 0.971, 1.071 |
| primitive-string-method | 0.982, 0.928, 0.990 |
| proto-method | 0.974, 0.932, 0.928 |

Stable and Rust 1.85 all-feature tests, fmt/Clippy, the eight existing pinned
Test262 regression selections, and the source revision's four CI and three
security jobs passed. Skip/ignored full audits are not counted as passing.
Broader conformance and the objective of beating actual omoikane Boa remain
unproven and open.

## Completed hosted and Boa comparisons

The [hosted run](https://github.com/ieee0824/temiya/actions/runs/40439763739)
used Rust 1.99.1 on x86-64 Intel Xeon Platinum 8573C (four logical CPUs),
with the same original eleven inputs and fixed timing/resource protocol.
All 66 runs / 1056 values passed in each mode. Hosted before/after classifications
were all 33 within noise; there is no confirmed hosted invocation speedup.
All 33 Boa comparisons were slower beyond threshold, with Temiya/Boa ratios
1.625–4.127. Both all-eleven speed gates are false.

The completed local ARM64 Boa mode also passed all 66 runs / 1056 values,
but all 33 comparisons were slower beyond threshold (ratios 1.467–3.925).
Do not infer cross-machine speed changes from these absolute timings.

- [Hosted before/after raw data](../benchmarks/empty-block-hosted-beforeafter.json)
- [Hosted Boa raw data](../benchmarks/empty-block-hosted-boa.json)
- [ARM64 Boa raw data](../benchmarks/empty-block-arm64-boa.json)
- [Source, build, artifact and verification provenance](../benchmarks/empty-block-hosted-provenance.json)

All raw samples, startup/setup observations, medians/MAD and classifications
are retained. Startup remains outside invocation samples. This report adds
measurements to the already merged optimization; it does not complete the
overall performance issue or establish Boa superiority.
