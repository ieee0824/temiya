# Public instance field inputs

These inputs were frozen before the #157 runtime implementation. The fixed selection contains all
2057 files whose pinned reference metadata includes class-fields-public. Cases
with other unsupported dependencies remain in the selection.

The five authored workloads cover base/default construction, derived fields,
computed keys evaluated once per class, own data definition bypassing inherited
setters, and per-instance arrow initializers. Each performs10000 constructions
and returns50005000 with extra state assertions. All five were checked twice
against the actual pinned Boa worker before runtime implementation; the archive
retains20 validated warmup/sample values. These are oracle probes, not timing
acceptance. Performance qualification must retain the existing15 samples, three
alternating pairs and noise rule, plus independent controls and host results.

initial-v1 retains the first implementation and both4098-variant captures per
backend, including all failures/timeouts and429 added passes with0 prior-pass
regressions. Its native Boa/self controls have5wins10noise and15noise. This
source is not merge-qualified. verify-initial.mjs replays that exact history.

field-capture-v1 retains the preceding optimized source f2b0083, all fixed
4098-variant captures for both backends, and four native performance reports.
verify-field-capture.mjs checks archive integrity and independently reconstructs
the correctness and timing records. Its 13 Boa wins and two noise outcomes,
15 improvements against the initial implementation, and 15 identical-binary
noise outcomes are historical evidence, not final-source qualification.

dependency-probes-v2 retains standalone reproductions for existing static/private
class elements, eval, Symbol construction, Promise.resolve and async-generator
issues. Basic async completion and well-known Symbol controls pass. These probes
do not convert any official-suite failure to a pass. The rejected v1 probe setup
omitted doneprintHandle.js and is not used as engine evidence. run-probes.mjs is
the original local capture driver; its absolute paths identify the captured
engine and corpus rather than a portable test command.

native-asi-v1 retains the final parser-fix ARM measurements: all fifteen Boa
comparisons exceed the fixed threshold; identical-binary and preceding-source
controls are all within noise. verify-native-asi.mjs checks all 77 compiled
source hashes against the measured revision, raw statistics and 827 passing
native tests. Hosted and full-corpus qualification remain separate.

full-fields-v1 replays all eight official corpus shards at source3f9cbec:
26826pass,64484fail,18skip,8883timeout over100211variants, with no known-pass
regressions. hosted-initial-fields-v1 retains all40reports from two independent
EPYC7763 hosts. Copy1 arrow initializer has0/3significant Boa wins and fails
the fixed minimum2/3gate; copy2 has2/3. This source is not merge-qualified.
verify-full.mjs and verify-hosted.mjs preserve these respective results.

native-cache-v1 retains the following cache-only source835b21d. It has12Boa
wins and3noisy arrow comparisons. Identical-executable controls retain one
apparent improvement and one slowdown; neither is a source optimization.
verify-native-cache.mjs checks the complete records and828native passes.
This historical source is not speed-qualified.
