# RegExp compilation cache evidence

Partial evidence for PR #515 / issue #513. Hosted two-copy x64 run 40584186976
and final validation are pending. No overall performance-goal completion is
claimed. See [method, results and limits](../../docs/regexp-compilation-cache.md).

`variants.json` identifies every measured source, executable hash and compiler.
Losslessly compressed patches reconstruct each variant against main 3f9d1ae.
All predecessors remain available, including slower results:

- `lightweight`: no performance features; construction still slower than Boa,
  global execution regresses against main.
- `selective`: bounded-backtracking/inline features, no compilation reuse;
  construction still slower than Boa.
- `cache`: full-feature matcher with compilation reuse; still slower than Boa
  for construction and one Unicode before/after regression.
- `cache-selective`: lightweight compilation reuse; all nine Boa comparisons
  faster, but one global-exec before/after comparison regresses.
- `cache-captures`: additionally reuses each object's capture storage; all nine
  Boa comparisons faster, other execution inputs within noise against main.
  The original eleven inputs have 33 before/after noise and 33 Boa wins.

`*-verification.json` retains local verification of exact fixtures, protocol,
actual frozen executables, real application fork/lock/compiler and independently
recalculated statistics. Frozen workers are not distributed. The historical
actual Boa adapter source identity is documented in the existing typed-number
evidence; it was not rebuilt or relabeled for this experiment.

`cold-unique/` records a separate 2,048-construction input with fresh pattern text
across every pass, all checked outputs and sampled memory. It is not a substitute
for the fixed authored suite. **All three cold/unique comparisons remain slower
than actual Boa**. `runner-snapshot.mjs` preserves the local diagnostic runner,
including its original temporary paths; it is a source snapshot, not a portable
worker build script. First cold pass timings remain in each raw measurement.

`cache-public-and-test262.log.gz` retains the initial test-harness failure (setting a
native object's own toString property did not install its embedding conversion
hook). The corrected test uses the public set_to_string_hook API; the final compressed log
records both native cases and all 64 fixed Test262 passes. The original library
eviction test is covered by Miri; the large-state byte-budget stress runs natively.

Offline verification, without building or timing engines:

```sh
node benchmarks/regexp-cache/verify-recorded-timing.mjs benchmarks/regexp-cache
node benchmarks/regexp-cache/verify-cold-unique.mjs
node benchmarks/typed-number/verify-integrity.mjs benchmarks/regexp-cache
```

There are currently 12 fixed-suite reports, 312 workers, 4,992 checked values and
156 pair decisions, plus the separate cold/unique diagnostic's nine workers,
144 checked values and six comparisons. `inventory.json` hashes every stored
file except itself and also hashes decoded gzip payloads. Memory observations
are sampled process HWM, including startup/setup/warmup; null is not zero.
