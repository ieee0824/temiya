# Whole-Script Auto integration

This integration makes
Engine::default, Engine::new and the default builder choose Auto. A supported
whole Script executes as verified bytecode; an unsupported Script is selected
for AST execution before any observable effect. Runtime errors are propagated
once and never trigger an AST retry. Explicit Ast/Bytecode configuration remains
available. Modules retain their existing module path.

The AST-function loop tier proposed in #491 is excluded: AST function creation
and calls remain the existing main implementation. There is no new TLS loop
policy, lazy loop compilation or per-AST-function cache. The old #492 performance
observation is retained and is not declared resolved by these source changes.
Supported functions in a selected VM Script still use verified VM function bodies.

The integrated runtime includes checked local binding/property hints, primitive
Number/String operations, guarded ordinary Array push, precise arguments-object
observation, prepared parameter records, local compound/update operations, inline
small call arguments and direct call-target reads. Regressions retain TDZ/const,
callback reentry, Proxy/getter order, receiver snapshots, iterator errors and
arguments/constructor behavior. Unsupported language forms remain unsupported;
this is not a whole-spec or GC completion claim.

The benchmark worker uses Engine::default and records actual setup/invocation
backends. Setup and invocation Script destruction occur before their respective
elapsed timestamps. Reports use mode Auto Script; the older experimental
Auto Script+loop reports are different source combinations and are not final
performance acceptance for this branch.

## Measured default API

Measured source 351de1ce4f69c8f60d89a3cae7054e41fecc1c28 uses Engine::default,
without the AST-function loop tier. Every Temiya worker reports Bytecode for
both setup and invocation. Counts below are faster / within noise / slower
using the fixed max(0.10, 3 * (MADa/mediana + MADb/medianb)) threshold.

| Machine | Before/after | Actual omoikane Boa |
| --- | --- | --- |
| hosted x64 | 33 / 0 / 0 | 33 / 0 / 0 |
| ARM64 | 33 / 0 / 0 | 33 / 0 / 0 |

All four all-eleven gates pass. Hosted baseline is AST at
2603ecb0ec01ecdbf625f94daf80b2ec9917658d; ARM64 baseline is AST at
a23e82b8cd8230e4924747e5f09a55007024e550. These baseline runtimes are identical;
the intervening changes retain measurement records only.

[Hosted run](https://github.com/ieee0824/temiya/actions/runs/40519128128) and the
ARM64 run retain the original eleven workload bodies, iteration counts and
oracles from omoikane 007556adccb8fb4329236932ca8175f8e8d119d2, fifteen timed
samples plus one checked warmup, three alternating serial pairs, and common
512 MiB / 180-second guards. The actual Boa worker retains the application
embedding, lockfile and features. All builds finish before timing. ARM64 runs
Boa first, then before/after, with the same within-comparison protocol.

All 264 workers and 4,224 results passed validation; medians/MADs and all 132
pair decisions were independently recomputed. Original manifest/oracles, full
unique coverage, limits, modes, actual backends, compiler/build identities,
exact hosted workflow head/artifact and local executable hashes were verified.
[Provenance](../benchmarks/default-auto-provenance.json) records source/baseline,
worker identities, run/artifact links and raw report hashes. Raw files use
default-auto-hosted or default-auto-arm64 plus -beforeafter.json or -boa.json.

Source CI40517251027, PR CI40521487933 and security40519122873/40521488447 passed.
ARM64 release all-features tests passed; opt-in corpus tests not executed locally
are not counted as passes. The following evidence/documentation commit changes
no runtime, worker or comparison implementation; final-head CI is rerun.

This is a result for the original eleven workloads on these machines, not a
whole-application or every-program speed claim. Earlier experimental regressions
remain recorded. The [coverage matrix](benchmark-suite-coverage.md) links later
source-specific async/RegExp/phase/SpiderMonkey evidence and the remaining
Engine collection and application/compatibility limits under #427.
