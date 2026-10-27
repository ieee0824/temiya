# Array length and compound binding comparisons

Counts are faster / within noise / slower under the fixed threshold
max(0.10, 3 * (MADa/mediana + MADb/medianb)). These records preserve completed
comparisons regardless of outcome and do not adopt runtime changes.

| Candidate and machine | Before/after | Actual omoikane Boa |
| --- | --- | --- |
| array-length-hosted | 33 / 0 / 0 | 22 / 9 / 2 |
| compound-binding-hosted | 33 / 0 / 0 | 27 / 5 / 1 |
| array-length-arm64 | 26 / 7 / 0 | 12 / 21 / 0 |

All candidates explicitly select Auto Script+loop. The hosted baseline is
production AST at a0ca0994e25cbaa56ab2a015e9c9cbbf0f514993; ARM64 uses AST at
762d295606578230376b7ce186d4a68f41f9b03e. These baselines have the same runtime;
the intervening merged changes only retain measurement records. Each candidate
is a cumulative implementation combination, so these comparisons do not isolate
the causal effect of any one optimization or resolve the original #492 regression.

The array-length candidate includes the arguments-observation fix, bulk parameter
initialization, ordinary own-data length reads and earlier experiments. It still
has two slower hosted array pairs (ratios 1.141997 and 1.158122, threshold 0.10).
The compound-binding candidate additionally reads/writes known local slots for
nonlogical compound assignments, retaining the old value across RHS evaluation
and coercion callbacks. Its hosted array pair 1 remains slower: ratio 1.107868,
threshold 0.10; the other array pairs are within noise. A median below one is not
a threshold-qualified win. Neither candidate passes the all-eleven Boa gate.

The arguments-object scan correction in db1713bc85d7a93883df77c7e2ceee04da8b1547
is included in both candidates; earlier buggy candidates documented in
[string and Number checkpoints](string-number-checkpoints.md) remain separate.
Final source CI passed for [array length](https://github.com/ieee0824/temiya/actions/runs/40501913311)
and [compound bindings](https://github.com/ieee0824/temiya/actions/runs/40503734763).
These checks do not by themselves establish performance acceptance.

Each report retains all eleven original workload bodies, iteration counts and
oracles from omoikane 007556adccb8fb4329236932ca8175f8e8d119d2, fifteen timed
samples, one checked warmup and three alternating serial pairs. The actual Boa
worker uses the pinned application's embedding, lockfile and features. Both
workers retain the same 512 MiB / 180-second process guards. Timing includes
invocation parse, evaluation and Script destruction; startup/setup/warmup are
excluded. CPU/compiler details and exact modes remain attached to each report.

All six reports were validated: 396 successful workers, 6,336 checked results,
and 198 independently recalculated pair decisions. Original manifests/oracles,
complete unique coverage, guards, modes, compiler and build identities were
checked. Hosted workflow head SHAs and artifact IDs were matched to GitHub;
local executable SHA-256 values were checked against retained binaries.
[Provenance](../benchmarks/call-array-provenance.json) records exact revisions,
report hashes, hosted runs/artifacts and build information. Raw filenames use
the table labels plus -beforeafter.json or -boa.json in benchmarks/.

The production backend remains AST and #427 remains open. Subsequent guarded
Array-push and call optimizations are not measured by these reports.
