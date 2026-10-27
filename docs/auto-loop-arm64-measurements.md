# Automatic function-loop bytecode: completed ARM64 comparison

The experimental runtime in #491 was compared using explicit `Auto Script+loop`
mode against the default AST interpreter. The changed source is
`1bbb33a73bed40e0930c8c27cf2876bfccec9603`; the baseline is
`b52201dcc9c0667bc1b691a9b95072b21173b54b`. The CPU model on this ARM64 machine
is reported as unknown. This comparison predates later experimental binding
reference and object-literal changes and does not measure those changes.

Both [before/after](../benchmarks/auto-loop-arm64-beforeafter.json) and
[actual Boa](../benchmarks/auto-loop-arm64-boa.json) retain the original eleven
workloads, 15 timed samples plus a checked warmup, three alternating pairs and
512 MiB / 180-second guards. Each report contains 66 successful workers and
1056 correct values. Every median, MAD, ratio, fixed threshold and outcome was
independently recomputed. Original manifest/oracles, worker coverage,
compiler and executable hashes were checked; see the complete
[provenance](../benchmarks/auto-loop-arm64-provenance.json).

The before/after comparison has 14 pairs faster beyond threshold, 18 within
noise and one slower pair. Prop-mono pair0 has after/before ratio
1.1135082443094682 with threshold 0.10. Its other two pairs have ratios
1.0040934778630741 and 0.9950483627708926, both within the 0.10 threshold.
The original flag is retained and investigated in #492; no isolated cause or
fix has been established. The prop-mono body contains an object literal, so
the experimental compiler cannot lower its whole body and selects the AST
path. This does not establish the cause of the measured slowdown.

The actual Boa comparison has 32 pairs slower beyond threshold and one within
noise. Neither all-eleven speed gate passes; this result does not establish
Boa superiority. The earlier hosted result has 24 faster before/after pairs,
9 within noise and no slower pair; it used a different CPU. Both observations
remain recorded in [the hosted comparison](three-experiment-comparisons.md).
#491 remains held for regression investigation, and #427 remains unmet.
