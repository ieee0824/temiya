# Completed AST function-code sharing measurements

These reports record experimental #481. They do not enable function-code
sharing on main or establish an overall throughput improvement. Regression
investigation remains open in #483; actual Boa superiority remains unmet (#427).

All comparisons use the original eleven omoikane workloads, 15 timed samples
plus one checked warmup, three alternating pairs, and the existing 512 MiB /
180-second watchdog. Each report has 66 successful workers and 1056 checked
values. Every median, MAD, ratio, fixed threshold and classification was
independently recomputed. Original manifest, result oracles, pair coverage,
compiler identity and worker hashes were checked. Full observations, including
unfavorable results, are retained; no pair was dropped or threshold changed.

[Provenance](../benchmarks/function-code-completed-provenance.json) records
exact revisions, machine details, build metadata, artifact run links and report
SHA-256 hashes. Local worker binaries were also rehashed. The runtime of
integrated measured head be7ba01 is da4dd1b; its parent comparison is main
67b49662. Isolated 10bf1e1 instead compares against d2d1282 and excludes the
later native-initialization change. These sources are different experiments.

| Source / environment | Before/after faster | Within noise | Slower | Actual Boa slower |
| --- | ---: | ---: | ---: | ---: |
| Integrated be7ba01 / ARM64 | 2 | 29 | 2 | 33/33 |
| Integrated be7ba01 / hosted x64 | 1 | 32 | 0 | 33/33 |
| Isolated 10bf1e1 / hosted x64 | 2 | 31 | 0 | 33/33 |

The integrated ARM64 regressions are string-concat pair1 (after/before
1.1821315617892476, threshold 0.10) and array pair2 (1.1669485864573965,
threshold 0.10778174234629365). All three closure-allocation pairs on that
machine remain within noise. Integrated hosted x64 has one faster
closure-allocation pair1 (0.8983385256823047, threshold 0.10). Different machines
and sources do not establish a cause or an isolated fix. All all-eleven speed
gates remain false. Allocation reductions alone do not prove latency or RSS
improvements.

Raw reports:

- Integrated ARM64: [before/after](../benchmarks/function-code-integrated-arm64-beforeafter.json), [Boa](../benchmarks/function-code-integrated-arm64-boa.json).
- Integrated hosted x64: [before/after](../benchmarks/function-code-integrated-hosted-beforeafter.json), [Boa](../benchmarks/function-code-integrated-hosted-boa.json).
- Isolated anchored hosted x64: [before/after](../benchmarks/function-code-anchored-hosted-beforeafter.json), [Boa](../benchmarks/function-code-anchored-hosted-boa.json).

Earlier e6 observations, including the ARM64 primitive-string regression,
remain retained on #481. They are not superseded or reclassified by these
later measurements. This record does not close #481, #483 or #427.
