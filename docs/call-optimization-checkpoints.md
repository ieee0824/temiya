# Array and call optimization checkpoints

These records retain completed experimental comparisons, including slow and
inconclusive outcomes. Counts mean faster / within noise / slower under the
fixed threshold max(0.10, 3 * (MADa/mediana + MADb/medianb)).

| Candidate and machine | Before/after | Actual omoikane Boa |
| --- | --- | --- |
| array-key-hosted | 33 / 0 / 0 | 23 / 8 / 2 |
| array-push-hosted | 33 / 0 / 0 | 31 / 1 / 1 |
| inline-arguments-hosted | 33 / 0 / 0 | 30 / 3 / 0 |
| direct-calls-hosted | 33 / 0 / 0 | 31 / 2 / 0 |
| array-push-arm64 | 31 / 2 / 0 | 15 / 18 / 0 |
| direct-calls-arm64 | 31 / 2 / 0 | 17 / 15 / 1 |

Every candidate explicitly uses Auto Script+loop. All hosted baselines use
production AST at dcce965dfa7c20514a8b4e0144c1b7aea63185e4. Array-push ARM64 uses
AST at 762d295606578230376b7ce186d4a68f41f9b03e; these two baseline revisions have
the same runtime. Direct-calls ARM64 instead uses AST at
a23e82b8cd8230e4924747e5f09a55007024e550, including the merged primitive Number
fast path. These baseline identities are distinct and preserved in provenance.
Candidates are cumulative combinations, not isolated causal measurements of
the feature named by each row. Session-to-session absolute times cannot establish
an optimization's contribution.

The guarded Array push path retains generic Set behavior for indexed prototypes,
Proxy/exotic receivers, readonly length, nonextensible additions and u32 length
overflow. Inline argument storage retains the existing spread/IteratorClose and
argument-limit path. Direct call targets preserve receiver snapshots, computed
key/getter/argument order and global name observability. Source CI and security
passed for these candidates, but those checks do not establish speed acceptance.

Important remaining results:

- Array-push hosted prop-mono pair 1 is slower than Boa: ratio 1.149428896,
  threshold 0.10. Its primitive String method pair 1 is within noise.
- Inline arguments' three remaining hosted outcomes are primitive String method
  pairs, ratios 0.921315 / 0.932909 / 0.915991, each within threshold 0.10.
- Direct calls' hosted primitive String method pairs 1 and 2 are within noise,
  ratios 0.907220 / 0.922919, threshold 0.10.
- Direct calls' ARM64 primitive String method pair 0 is slower: ratio
  1.390428376, threshold 0.332087444. This observation is retained without
  attributing it to environment differences or declaring a fix.

Each report uses the eleven original bodies, iteration counts and oracles from
omoikane 007556adccb8fb4329236932ca8175f8e8d119d2, fifteen timed samples plus one
checked warmup, three alternating serial pairs and 512 MiB / 180-second guards.
The actual Boa worker retains the application's lockfile, features and embedding.
Invocation parsing, evaluation and Script destruction are timed; startup/setup
and warmup are excluded from samples. ARM64 runs here perform actual Boa
comparison first, then before/after; within each comparison the protocol is
unchanged and the order is recorded in notes.

All twelve reports passed validation: 792 successful workers, 12,672 checked
results and 396 independently recomputed pair decisions. Original manifest and
oracles, complete unique coverage, limits, modes, compiler/build identities,
hosted workflow heads/artifact IDs and local executable hashes were verified.
[Provenance](../benchmarks/call-checkpoints-provenance.json) records exact source
and baseline revisions, report hashes, run/artifact links and build metadata.
Raw names use the table labels plus -beforeafter.json or -boa.json in benchmarks/.

No report passes the all-eleven Boa gate. Within-noise outcomes are not wins.
Production still defaults to AST; these experiments are not adopted by this
record. #427 and the original #492 regression investigation remain open.
The later primitive-Number bitwise candidate is not measured in these reports.
