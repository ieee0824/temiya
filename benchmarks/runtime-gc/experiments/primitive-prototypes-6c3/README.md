# Invocation-local primitive prototype facades, runtime source 6c33f6e

Each VM invocation lazily borrows at most five intrinsic prototype facades from
its fixed Realm. The immutable intrinsic registry already keeps these objects
reachable. Property values/descriptors remain live reads; object-key coercion,
receiver behavior, GC barriers and activation-root clearing remain unchanged.

Native tests: 659 passed, 21 ignored across 86 suites. Clippy, selected pinned
Test262, strict ownership/retained-handle probes and worker-contract checks pass.
The new exact public integration Miri test passes strict provenance and symbolic
alignment with isolation enabled; it covers five primitive types, live updates,
collection, nested entry into another Realm, A/B/A bytecode reuse and final
release. This single integration test is not full library Miri qualification.

Original eleven workloads, 15 samples and three alternating pairs, unchanged
oracles/counts, 512MiB and 180-second watchdogs, and the unchanged max(10%, three
times summed relative MAD) gate are retained. Each report validates 66 workers
and 1056 values. Workers run serially on local ARM64 guest CPU0; agent threads
use CPUs1–3. No local builds/tests/profilers overlap timing. Affinity is restored
afterward; guest affinity does not prove physical host isolation.

Before/after versus runtime872: all33 comparisons within noise. No qualified
improvement or regression is claimed. Actual pinned omoikane modified Boa:
32 faster and1 within noise (prop-mega pair2, ratio0.6981, threshold0.4795).
All primitive-string-method pairs pass the speed gate, but this report does not
qualify all33 comparisons. No runs or pairs are discarded. The [hosted follow-up](hosted-general) qualifies all33 actual-Boa decisions
on each of AMD EPYC9V45 and AMD EPYC7763, with every report validating66 workers
and1056 values. Main comparisons retain7 slower decisions per host; same-binary
controls retain33 noise and32 noise/1 slower. These are bounded host results,
not an application-wide claim or a revision of the local32/1 observation.
Current-source full library Miri qualifies119 distinct exact tests,0 ignored,
all16 shards/aggregate/ASan/audit successful at source4b256bd (run40773845352).
The complete log, API status and independent verifier output are in ci/. Earlier-source hosted,
component, async and full-Miri results retain their original source labels.

statistics-verifier.mjs independently recomputes medians, MAD, original-input
coverage, value counts, order, limits and decisions from either report. File
hashes and source object identities bind these observations to the source.

The source/compiler/machine-bound actual Engine collection profile and all
fifteen issue criteria are mapped in acceptance-follow-up.json and
phase-acceptance-map.json. The implementation merged through PR522 at
05ca830a5c217c27cf6e6e9e7c2d8542007950a2. The remaining capture-time gate is
integration of these qualification records and issue427 finalization.

Run `node benchmarks/runtime-gc/experiments/primitive-prototypes-6c3/verify-integrity.mjs`
from the checkout to verify the complete byte inventory and raw/compressed log
hashes. Statistical, build-identity and exact Miri audits remain separate.
