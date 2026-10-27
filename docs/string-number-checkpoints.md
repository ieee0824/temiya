# String and Number optimization checkpoints

These completed measurements retain experimental implementations, including
failures and inconclusive outcomes. Counts mean faster / within noise / slower
using max(0.10, 3 * (MADa/mediana + MADb/medianb)).

| Candidate and machine | Before/after | Actual omoikane Boa |
| --- | --- | --- |
| primitive-string-hosted | 33 / 0 / 0 | 6 / 21 / 6 |
| string-append-hosted | 33 / 0 / 0 | 12 / 18 / 3 |
| number-update-hosted | 33 / 0 / 0 | 23 / 7 / 3 |
| number-update-arm64 | 30 / 3 / 0 | 14 / 19 / 0 |

All candidates explicitly use Auto Script+loop; the baseline uses production AST
at 762d295606578230376b7ce186d4a68f41f9b03e. These are cumulative experimental
combinations, not isolated causal measurements of the named feature. Neither the
production default nor the implementation changes are adopted by this record.
The hosted Number-update candidate remains slower in all three array pairs.
Within-noise outcomes are not wins, and none passes the all-eleven Boa gate.
The original regressions tracked by #492 are not resolved by these comparisons.

Each of eight reports contains the original eleven workloads and iteration
counts from omoikane 007556adccb8fb4329236932ca8175f8e8d119d2, fifteen timed
samples plus one checked warmup, and three alternating serial pairs with the
same 512 MiB / 180-second process guards. The actual Boa embedding retains the
application lockfile and features. All 528 worker processes and 8,448 results
passed validation. All medians, MADs and 264 pair decisions were independently
recalculated. The original manifest/oracles, unique full coverage, compiler,
execution modes, exact revisions, build metadata and local executable hashes
were checked. Hosted head revisions were matched to the corresponding GitHub
runs. Timing includes invocation parsing, evaluation and Script destruction;
process startup, setup and warmup are excluded.

## Correctness limitation discovered after measurement

All four candidates include an unmerged optimization that elides an unused
arguments object. Its observation scan missed binding names in scope plans:
`function f(){var arguments;return arguments[0];} f(42)` incorrectly threw a
TypeError. Production AST returns 42 and is unaffected. These original eleven
workloads do not contain this case, so their measured results remain recorded,
but the candidates must not be adopted as correctness-qualified implementations.
The scan is corrected in db1713bc85d7a93883df77c7e2ceee04da8b1547, including
hoisted and nested-block var declarations and parameter/function shadowing
regressions. CI 40498515373 passed. Subsequent candidates include this correction;
these reports do not measure the corrected source.

[Provenance](../benchmarks/string-number-checkpoints-provenance.json) identifies
every hosted run/artifact, compiler, exact source/baseline and report SHA-256.
Raw reports are in benchmarks/ with the table label plus -beforeafter.json or
-boa.json. The broader goal #427 remains open.
