# Literal and binding follow-up measurements

These are completed comparisons, retained regardless of whether an optimization
passed. They contain no runtime changes. Exact measured/baseline revisions,
compiler, CPU, modes, worker hashes, report hashes and hosted artifact identities
are in [provenance](../benchmarks/literal-followup-provenance.json).

| Comparison | Before/after faster / noise / slower | Boa faster / noise / slower |
| --- | --- | --- |
| binding-reference-hosted | 1 / 32 / 0 | 0 / 0 / 33 |
| auto-object-hosted | 6 / 27 / 0 | 0 / 1 / 32 |
| auto-object-arm64 | 7 / 26 / 0 | 0 / 0 / 33 |
| loop-slots-arm64 | 3 / 30 / 0 | 0 / 0 / 33 |
| array-literals-hosted | 0 / 33 / 0 | 0 / 0 / 33 |

Binding-reference and Auto-object observations compare explicit Auto Script+loop
workers with the earlier Auto worker 1bbb33a73bed40e0930c8c27cf2876bfccec9603.
Auto-object combines binding References and object data lowering; it is not an
isolated test of either change. Loop-slots compares the checked function-position
cache with full literal Auto revision 71754bcee266db557154d3247fc1f3f32e62fa84.
Array-literals compares default AST revision 856b33ccf597ac12b8c6447b04e5c91e9af82913
with main eec364ea823abe85ae32ca709becef4b00c2664b; this predates the later main
integration and seventh original custom-iterator test. Its result does not stand
in for the final integrated array comparison. None includes the later global
binding-resolution correction (#497). Different source combinations/baselines do
not establish the cause or resolution of the isolated #492 regression.

Each report retains the same original eleven workloads and iteration counts from
omoikane 007556adccb8fb4329236932ca8175f8e8d119d2: fifteen samples, one checked
warmup and three alternating pairs, serial 512 MiB/180-second guards. Each contains
66 successful workers and 1,056 checked results. Actual Boa uses the pinned original
embedding, lockfile and feature selection with the same compiler. Statistics and
all 330 pair classifications were independently recalculated. Complete unique
coverage, original manifests/oracles, limits and build metadata were checked;
retained local executable hashes were rechecked. The decision threshold stays
max(0.10, 3 * (MADa/mediana + MADb/medianb)).

No report passes the all-eleven faster gate. Every candidate/Boa median ratio
exceeds one; within-noise comparisons are not wins. #427 remains open.

Raw files are in benchmarks/ with the comparison label above and suffix
-beforeafter.json or -boa.json. Hosted runs:
[binding](https://github.com/ieee0824/temiya/actions/runs/40470469641),
[Auto object](https://github.com/ieee0824/temiya/actions/runs/40472178461),
[array](https://github.com/ieee0824/temiya/actions/runs/40474776731).
