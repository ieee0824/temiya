# Numeric conversion, automatic loop bytecode and packed-number measurements

This record retains four completed comparisons. It does not merge the
experimental runtimes or claim overall Boa superiority. The original 11 pinned
omoikane workloads, 15 timed samples plus one checked warmup, three alternating
pairs and 512 MiB / 180-second guards were used. Each report has 66 successful
workers and 1056 validated values. Every median, MAD, ratio, fixed threshold and
outcome was independently recomputed; the original manifest, result oracles,
coverage, compiler identity and build metadata were checked. The ARM64 workers
were also rehashed. [Provenance and artifact IDs](../benchmarks/three-comparisons-provenance.json)
record exact source revisions, binaries and machines.

| Experiment | Environment | Before/after faster | Within noise | Slower | Actual Boa slower |
| --- | --- | ---: | ---: | ---: | ---: |
| Bounded integer conversion #485 | x64 EPYC 7763 | 0 | 33 | 0 | 33/33 |
| Auto Script and function loops | x64 EPYC 7763 | 24 | 9 | 0 | 33/33 |
| Packed numeric elements #489 | x64 EPYC 9V45 | 0 | 33 | 0 | 33/33 |
| Packed numeric elements #489 | ARM64, CPU unknown | 0 | 31 | 2 | 33/33 |

For automatic loop bytecode the worker explicitly selected `ScriptBackend::Auto`
and reports `Auto Script+loop`. The baseline uses the default AST interpreter.
The measured runtime revision is `1bbb33a73bed40e0930c8c27cf2876bfccec9603`,
based on the experimental runtime in `perf/auto-loop-bytecode`; the benchmark
worker and report script were changed only to select and identify that mode.
The hosted comparison finds all three pairs faster for arithmetic, polymorphic
property access, calls, closure allocation, string concatenation, primitive
string properties and methods, and prototype methods. The three remaining
workloads have all pairs within noise. Compilation of whole unsupported Scripts
falls back to AST, and eligible AST-created ordinary function loops compile on
first call. These results are from one machine and this explicit mode, not the
current default Engine. All actual Boa pairs still find Temiya slower.

The packed-number ARM64 regressions are call pair2 (1.1128864003856196,
threshold 0.10) and primitive-string-method pair2 (1.1494031587678586,
threshold 0.1440049303592037). Every array pair was within noise in both
architectures. The different hosted CPU did not reproduce these flags; that
alone does not establish their cause. #489 remains open while they are
investigated. The earlier #485 ARM64 object-allocation flag remains in its
[original record](int32-conversion-measurements.md), even though the x64
comparison has all 33 pairs within noise.

Raw reports:

- Integer conversion hosted: [before/after](../benchmarks/int32-hosted-beforeafter.json), [Boa](../benchmarks/int32-hosted-boa.json).
- Automatic loop bytecode hosted: [before/after](../benchmarks/auto-loop-hosted-beforeafter.json), [Boa](../benchmarks/auto-loop-hosted-boa.json).
- Packed numbers hosted: [before/after](../benchmarks/packed-numbers-hosted-beforeafter.json), [Boa](../benchmarks/packed-numbers-hosted-boa.json).
- Packed numbers ARM64: [before/after](../benchmarks/packed-numbers-arm64-beforeafter.json), [Boa](../benchmarks/packed-numbers-arm64-boa.json).
