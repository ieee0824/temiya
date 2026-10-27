# Optimization checkpoints

Completed comparisons are retained here regardless of outcome. No runtime change
is included. Counts below are faster / within noise / slower, using the fixed
threshold max(0.10, 3 * (MADa/mediana + MADb/medianb)).

| Comparison | Before/after | Actual omoikane Boa |
| --- | --- | --- |
| loop-slots-hosted | 0 / 33 / 0 | 0 / 0 / 33 |
| number-binary-hosted | 27 / 6 / 0 | 0 / 0 / 33 |
| corrected-number-hosted | 33 / 0 / 0 | 0 / 8 / 25 |
| unused-arguments-hosted | 33 / 0 / 0 | 4 / 8 / 21 |
| fused-update-hosted | 33 / 0 / 0 | 3 / 12 / 18 |
| integer-remainder-hosted | 33 / 0 / 0 | 3 / 16 / 14 |
| named-property-hosted | 33 / 0 / 0 | 8 / 12 / 13 |
| own-property-hosted | 33 / 0 / 0 | 6 / 18 / 9 |
| indexed-property-hosted | 33 / 0 / 0 | 9 / 18 / 6 |
| corrected-number-arm64 | 32 / 1 / 0 | 2 / 9 / 22 |
| integer-remainder-arm64 | 33 / 0 / 0 | 2 / 17 / 14 |
| indexed-property-arm64 | 30 / 3 / 0 | 3 / 27 / 3 |

The number-binary-only candidate and its baseline both use default AST. The
loop-slots hosted comparison uses explicit Auto Script+loop on both sides. All
other rows compare an experimental explicit Auto candidate with default-AST main
at the exact revision in the provenance record. The 704e33e and 674528b baselines
are retained separately; these are different source combinations, not isolated
measurements of each feature. CPU/compiler details and exact modes are recorded
per report. The loop-slots candidate predates the global binding-resolution fix;
the number-only candidate does not include it either. Later combined Auto rows
include the correction, but do not establish the cause or resolution of #492.

Each report uses all eleven original workload bodies, iteration counts and
oracles from omoikane 007556adccb8fb4329236932ca8175f8e8d119d2, fifteen timed
samples, one checked warmup and three alternating pairs, serial 512 MiB/180-second
guards. The actual Boa worker retains the pinned application embedding, lockfile
and feature selection. There are 24 reports, 1,584 successful worker processes
and 25,344 checked results. All 1,584 medians/MAD values and 792 pair decisions
were independently recalculated; complete unique coverage, original manifests
and oracles, guards, modes, compiler and build metadata were checked. Hosted
measured revisions were additionally matched against GitHub workflow head SHAs.
Local executable hashes were checked against the retained binaries.

[Provenance](../benchmarks/optimization-checkpoints-provenance.json) links every
hosted run/artifact, exact candidate/baseline, worker identity and report hash.
Raw reports use each row label above plus -beforeafter.json or -boa.json in
benchmarks/. No report passes the all-eleven faster-than-Boa gate. A median below
one or a within-noise outcome is not a threshold-qualified win. #427 remains
open. These results neither change the production default backend nor adopt
the experimental implementations.
