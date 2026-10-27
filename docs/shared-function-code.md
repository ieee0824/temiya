# Shared AST function code

Each Script owns immutable syntax and a cache of immutable Function snapshots.
Closures share snapshots while retaining independent environments, this, kind,
home objects, function identities, own properties and prototypes.

The cache retains its original Program and never removes cached Function
snapshots. A definition borrowed from either therefore has a stable address for
the cache's lifetime. That address is a lookup key and is never dereferenced.
This avoids repeated syntax equality checks; public AST span collisions do not
identify different definitions as equal. Copied generator continuation bodies,
modules/explicit VM adapters and direct public evaluators use owned code without
this cache. Future synthetic AST paths must retain that exclusion or supply a
valid immutable owner.

An escaping shared function keeps the whole Script syntax alive. The cache
holds syntax, not environments, Realms or callbacks. This is a code ownership
tradeoff, not a collecting heap or a claim of reduced retained memory.

Six public Engine/ScriptRecord regressions cover independent captures and
prototypes, recursion/defaults/this, Script destruction, re-evaluation, generator
body destruction, and colliding spans. Local release tests also cover existing
function, generator, native prototype and host-lifetime behavior. The isolated
anchored 10bf1e1 revision passed stable/MSRV CI and all security jobs; combined
source da4dd1b integrates the already merged native initialization optimization.

## Allocation observations

[Complete public Engine allocation probe](../benchmarks/function-code-anchored-allocations.json)
compares merged main67b49662 to combined da4dd1b. After 128 warmups, 1000
closure creations and checked calls use26000→17000 allocation requests and
4142000→3450000 requested bytes. Both paths validate1128 results including
warmup. Release targets and source probes are isolated/identical. Parsing and
Realm/setup creation are excluded. These are not timing or RSS observations.

## Historical e6 implementation

The earlier e6f6028 version compared entire Function syntax before sharing.
These four completed original-eleven-workload reports remain historical
evidence, not measurements of the latest anchored implementation.

- [ARM64 before/after](../benchmarks/function-code-e6-arm64-beforeafter.json):
  31 pairs within noise, one faster and one slower beyond threshold.
- [ARM64 actual Boa](../benchmarks/function-code-e6-arm64-boa.json):
  29 slower beyond threshold and four within noise; those four still have
  median ratios greater than1, with wide noise thresholds.
- [Hosted before/after](../benchmarks/function-code-e6-hosted-beforeafter.json):
  all33 pairs within noise on AMD EPYC9V45 x86-64.
- [Hosted actual Boa](../benchmarks/function-code-e6-hosted-boa.json):
  all33 pairs slower beyond threshold.
- [Build/artifact/statistical provenance](../benchmarks/function-code-e6-provenance.json).

Every report retains the original eleven inputs,15 samples plus checked warmup,
three alternating pairs,512MiB/180s guards,66 successful workers and1056 checked
values. Every median/MAD/ratio/threshold/outcome was independently recomputed.
All all-eleven speed gates are false.

The ARM64 primitive-string-method pair1 is slower by11.4386%, beyond its10%
threshold. Other two pair ratios0.907758 and1.067040 are within their own
thresholds. The hosted result on another CPU does not identify a cause or an
isolated fix. The workload loop calls native charCodeAt and does not create
Function syntax per iteration. Code layout/runtime effects and measurement
variation remain hypotheses. The flag is retained and tracked rather than
reclassified or hidden.

The anchored and integrated source has separate ongoing performance
comparisons. No confirmed latest-runtime speedup or Boa superiority is claimed.
