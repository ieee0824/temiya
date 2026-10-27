# Full literal integration measurements

These observations retain an experimental combination of Auto function-loop
lowering, binding References, object data literals and array literals. Measured
revision: 71754bcee266db557154d3247fc1f3f32e62fa84. Baseline: main revision
eec364ea823abe85ae32ca709becef4b00c2664b, using its default AST interpreter.
The candidate explicitly selects Auto; this is not the default production mode.
All candidate setup and invocation programs report Bytecode. The experiment
does not include the subsequent global name resolution correction (#497).

| Machine | Faster than main | Within noise | Slower than main | Faster than Boa | Within noise vs Boa | Slower than Boa |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Hosted x86_64 | 32 | 1 | 0 | 0 | 1 | 32 |
| Local ARM64 | 30 | 3 | 0 | 0 | 2 | 31 |

Each comparison retains the original eleven omoikane workloads, three alternating
pairs, fifteen timed samples and one validated warmup. Each report contains 66
successful workers and 1,056 validated results. The Boa reference uses the actual
omoikane embedding at 007556adccb8fb4329236932ca8175f8e8d119d2 with its original
lockfile and feature selection. Parse/evaluate invocation timing includes compiled
Script destruction and excludes setup/startup. Limits remain 512 MiB and 180 seconds.
The fixed noise threshold is max(0.10, 3 * (MADa/mediana + MADb/medianb)).

All candidate/Boa median ratios exceed one on both machines. Within-noise cases
are not wins. Neither the all-eleven main comparison gate nor the Boa superiority
gate passes. These measurements do not close #427 or establish resolution of the
isolated #492 regression; the integrated source and baseline differ from that test.

[Hosted run](https://github.com/ieee0824/temiya/actions/runs/40474729919),
artifact 12720334512. Raw reports: [hosted main](../benchmarks/full-literal-hosted-beforeafter.json),
[hosted Boa](../benchmarks/full-literal-hosted-boa.json),
[ARM64 main](../benchmarks/full-literal-arm64-beforeafter.json),
[ARM64 Boa](../benchmarks/full-literal-arm64-boa.json).
[Provenance](../benchmarks/full-literal-provenance.json) records exact revisions,
compiler, machines, worker hashes and report hashes. Statistics were independently
recalculated from all samples; original manifests/oracles, unique coverage, limits,
backend metadata and artifact build metadata were checked. Local executable hashes
were rechecked against the retained workers. No runtime change is included here.
