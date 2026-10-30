# Derived class measurements

`workloads-v1` freezes five authored inputs before their first execution in either
engine: explicit construction, implicit argument forwarding, inherited method
calls, a multilevel default-constructor chain and repeated derived definitions.
Counts, expected numeric values and source hashes are fixed in the manifest and
`scripts/derived-class-benchmark-reference.mjs`. No result is yet claimed here.

Use the existing Linux process/watchdog protocol with the derived suite selected:

```sh
TEMIYA_REVISION=SOURCE_SHA node scripts/compare-class-workloads.mjs --derived \
  benchmarks/derived-classes/workloads-v1 WORKER BOA_INFO 15 3 > REPORT.json
node scripts/audit-class-workload-records.mjs --derived REPORT.json
```

`--derived --temiya-baseline` selects a release Temiya before/after or identical
executable control. Actual Boa uses the unchanged pinned omoikane embedding.
Every warmup and sample must return the checked numeric value. Keep all three
alternating pairs, errors, timeouts, noisy and unfavorable decisions. The same
180-second/512-MiB watchdog, 15 samples, 10% noise floor and three-times relative
MAD rule apply. Setup/startup are excluded; invocation parse/evaluate is timed.
The original eleven and unchanged base-class four remain separate controls.
Neither validity nor a passing authored example proves speed superiority.
