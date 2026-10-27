# Fresh RegExp compilation observations (draft)

These observations do not establish completion of #516. Main 5616425 contains
the warm-cache improvement from #515; draft #517 is the candidate tested here.

The exact input is ../regexp-cache/cold-unique/input.json (SHA-256
66911cc92b9cf12141fa42a66f21176457ec148cd3a09aa829eba22f4b228ce9).
Each cold report contains three workers in three serial rotations, 15 timed
samples plus one checked first pass, 144 checked results and six decisions.
The reference is the actual modified Boa embedded by omoikane
007556adccb8fb4329236932ca8175f8e8d119d2, not stock Boa or an adapter substitute.
Local frozen adapter provenance differs by formatting only as documented in
../regexp-cache; hosted workers build the canonical adapter against the real app.
Local absolute paths identify historical files, not portable executable locations.

## Results and source identity

local/ retains all twelve successive local cold reports, per-source build metadata
and compressed production patches against main 5616425. None passes all three
actual-Boa speed comparisons. Source and binary hashes must remain distinct.

hosted/initial-x64-* are run 40597703258, runtime equivalent to 7e75280;
both AMD cohorts pass all three Boa comparisons and repeat comparisons.
hosted/final-x64-* are run 40598960611, runtime equivalent to a8a824d;
both cohorts miss the all-three gate. hosted/final-arm64-* are run 40599864531,
same runtime a8a824d, and both cohorts miss all six total Boa decisions.
All hosted same-binary controls have within-noise candidate/control decisions.
The reference repetitions in those same reports are separate Boa comparisons.
Early artifacts are complete cold reports; hosted macro runs remain in progress.

additional/ retains affinity/same-binary diagnostics, four local macro reports,
profile attribution proof and quality logs (including initial Clippy failures).
The local original-eleven before/after report includes object-alloc pair 0
ratio 1.133801, classified slower. Its cause is unresolved. The completed same-binary control has 29 within-noise, two faster and two
slower decisions (call and string-concat). All three object-alloc controls
are within noise; this does not establish the absence of a source regression.
The local original-eleven Boa report is 31 faster/two within noise; authored
warm RegExp is five faster/four within noise. No whole-engine victory is claimed.

## Offline verification

Run node verify-cold-report.mjs PATH_TO_COLD_REPORT from this directory.
It independently recalculates median, MAD, ratio, unchanged threshold and
decisions; validates fixed input hash, rotations, checked counts, observed
Bytecode execution and actual fork provenance. A valid report with a false
speed gate remains false. This does not rebuild historical binaries or prove
whole-application throughput, full conformance, or process RSS bounds.
Compressed logs and patches preserve original bytes, including whitespace.

Final-profile run 40601358312 retains raw Callgrind and provenance in
profile-final/. The production source equals 0cdb99c; 323344296 instruction
references include setup/startup/two checked passes. Compiler inclusive cost
is 88125528 references; these counts overlap and are not elapsed-time wins.

The subsequent inline-scratch variant 40a4f58 removes one per-object heap
allocation. Its fixed-input local report is valid but has only one faster
and two within-noise Boa decisions, so its overall speed gate remains false.

Final x64 copy1 (Intel8370C, runtime a8a824d) has complete macro observations:
original-eleven candidate/Boa and baseline/Boa are each33 faster, before/after
and same-binary each33 within noise, warm RegExp/Boa9 faster, warm before/after
3 faster and6 within noise. Its cold gate remains false. These observations
do not qualify subsequent sources or the exact Intel6973 target in #510.

Property-hint variant831dc98 remembers only a validated ordinary-property
position, never a descriptor/value. Bounds/full-key checks preserve deletion
and redefinition; exotic/proxy handling still precedes this path. Native
all-feature release checks have529 passes in83 suites,19 ignored, with64
selected RegExp Test262 executions separately passing. Its local exact cold
report has three Boa decisions within noise and does not pass the speed gate.
