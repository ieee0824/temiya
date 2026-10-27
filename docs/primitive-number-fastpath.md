# Primitive Number binary fast path

When both operands are already Number primitives, arithmetic (+, -, *, /, %),
relational and equality operators directly compute their f64 result. No
JavaScript coercion callback can run for these operand pairs. Objects, mixed
types, BigInts and other operators keep the existing conversion path. AST and
bytecode share the implementation; the production default remains AST.

An independent Node v24.14.1 oracle records 1,053 combinations of edge Numbers
and thirteen operators. Both AST and bytecode check all cases using SameValue,
including signed zero, NaN and infinities: 2,106 observations. Existing mixed
conversion and exception regression tests continue to cover the fallback path.

## Final integrated measurement

[Hosted run 40503847770](https://github.com/ieee0824/temiya/actions/runs/40503847770)
compared source 18452b3f49180fcc182c9b03cddf012f1214cf75 against main
at a0ca0994e25cbaa56ab2a015e9c9cbbf0f514993. Both use default AST. Each report
retains all eleven original omoikane workload bodies, iteration counts and
oracles at 007556adccb8fb4329236932ca8175f8e8d119d2: fifteen timed samples plus
one checked warmup, three alternating serial pairs, 512 MiB / 180-second guards,
and the same compiler with isolated release targets. All builds finish before
timing. Startup/setup/warmup are excluded; each invocation includes parsing and
evaluation. The actual Boa worker uses the pinned application embedding,
lockfile and feature selection.

Before/after results are **23 faster, 10 within noise, 0 slower** using the fixed
threshold max(0.10, 3 * (MADa/mediana + MADb/medianb)). The all-eleven before/after
gate is false because within-noise outcomes are not wins. Actual Boa comparison
results are **0 faster, 0 within noise, 33 slower**. This is an incremental Temiya
improvement and does not satisfy the user's Boa goal or close #427.

Both reports contain 66 successful workers and 1,056 checked results, totaling
132 workers and 2,112 values. All medians/MADs and 66 pair decisions were
independently recomputed. Original manifests/oracles, complete unique coverage,
guards, modes, compiler/build metadata, exact revisions and the hosted workflow
head were verified. [Provenance](../benchmarks/primitive-number-provenance.json)
records run/artifact links, worker identities and raw-report SHA-256 values.
Raw data: [before/after](../benchmarks/primitive-number-final-hosted-beforeafter.json)
and [actual Boa](../benchmarks/primitive-number-final-hosted-boa.json).

Source CI [40503799707](https://github.com/ieee0824/temiya/actions/runs/40503799707)
and security [40503843150](https://github.com/ieee0824/temiya/actions/runs/40503843150)
passed for the measured runtime. The subsequent commit only adds measurement
records and this document; it does not change measured runtime or worker source.
