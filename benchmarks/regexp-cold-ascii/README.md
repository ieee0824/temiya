# Fresh RegExp compilation observations (draft)

These observations do not establish completion of #516. Main 5616425 contains
#515's warm-cache improvement; draft #517 is the candidate tested here.

The exact input is ../regexp-cache/cold-unique/input.json, SHA-256
66911cc92b9cf12141fa42a66f21176457ec148cd3a09aa829eba22f4b228ce9.
Each cold report has three workers in three serial rotations, 15 timed samples
plus one checked first pass, 144 checked results and six decisions. The minimum
relative improvement is 10%, raised by the existing MAD rule when needed.
The reference is the actual modified Boa embedded by omoikane
007556adccb8fb4329236932ca8175f8e8d119d2. Hosted workers build the canonical
adapter against the real app. The frozen local adapter differs by formatting
only, as documented in ../regexp-cache; absolute local paths identify historical
files, not portable executable locations.

## Source-qualified results

| Evidence | Production source | Cold speed acceptance | Original-eleven and warm regressions |
| --- | --- | --- | --- |
| Thirteen local candidates | Per-directory build metadata | Latest 6702d00 passes all three; earlier local candidates do not | Latest source has 529 native passes; hosted final-source regressions remain pending |
| initial-x64-copy1/2, run 40597703258 | 7e75280 | Both AMD cohorts pass, including repetitions | These early results do not qualify later source |
| final-x64-copy1/2, run 40598960611 | a8a824d | Both cohorts miss the all-three gate | Both complete; original candidate/Boa and baseline/Boa each 33 faster, before/after and identical-binary each 33 within noise; warm/Boa 9 faster per host |
| final-arm64-copy2, run 40599864531 | a8a824d | All initial Boa decisions within noise | Complete; original and warm results match x64 copy 1's categories |
| final-arm64-copy1, same run | a8a824d | All initial Boa decisions within noise | Complete; same macro categories as ARM64 copy 2 |
| inline-scratch-arm64-copy1/2, run 40602102161 | 40a4f58 | Both cohorts miss every initial and repeated Boa decision | This short diagnostic does not contain macro regressions |

The final x64 hosts are Intel 8370C and AMD 7763. Intel 8370C is not the exact
6973 target required by #510. All recorded hosted cold identical-executable
candidate/control decisions are within noise. Their separate Boa repetitions
remain separate speed comparisons.

The inline-scratch ARM64 initial Boa ratios are 0.960899/0.951186/0.966306
and 0.951701/0.937011/0.947955. The property-position candidate
831dc98 has a correct local report with all three Boa decisions within noise;
run 40602762774 is complete on two ARM64 and two x64 runners. Both x64
cohorts pass all initial and repeated Boa decisions (ratios 0.856–0.879).
ARM64 copy 1 has initial ratios 0.911495/0.891469/0.900603 (two within noise);
copy 2 has 0.890274/0.892919/0.888439 (three faster), but its repetition
includes 0.900314 within noise. All identical-binary controls are within noise.
The ARM64 condition remains incomplete; no passing cohort replaces a miss.
This short diagnostic does not contain required final-source macro validation.

Local a8 macro results retain a before/after object-alloc slowdown of 1.133801.
The exact same executable used for both control labels gives 29 within-noise,
two faster and two slower decisions (call and string-concat); object-alloc
controls are all within noise. Process variability is demonstrated, but it
does not establish that the source regression is absent. Local original/Boa
has 31 faster and two within noise; warm/Boa has five faster and four within
noise. No negative observation is dropped or relabeled.

## Contents and verification

local/ retains all successive cold reports, source/build identities and
compressed production patches against main 5616425. hosted/ preserves raw
workflow artifacts and source-equivalence proofs. additional/ includes
same-binary/affinity diagnostics, local macro reports and quality logs,
including initial Clippy failures. profile-final/ retains complete raw
Callgrind output for source 0cdb99c: 323344296 instruction references include
startup/setup and two checked Bytecode passes. Inclusive compiler cost is
88125528; overlapping instruction counts are not native elapsed-time wins.

Run node benchmarks/regexp-cold-ascii/verify-integrity.mjs from the repository
root. It verifies every recorded file's SHA and independently recalculates
cold and macro statistics, fixed oracles, checked counts and decisions.
A valid report with a false speed gate remains false. For a single cold report,
run node benchmarks/regexp-cold-ascii/verify-cold-report.mjs PATH.

The inventory covers 370 files, 45 cold reports and 29 macro reports. Macro
coverage is 1434 workers, 22944 checked results and 717 decisions. This is an
archive verification, not a new timing experiment, binary rebuild, whole-app
throughput claim, full-conformance claim or hard process-RSS bound. Compressed
logs and patches preserve their original bytes, including whitespace.

The single-borrow ordinary lookup candidate 6702d00 passes all three local
Boa comparisons, ratios 0.837020/0.834102/0.850284, under the unchanged
noise thresholds. Full native release/all-features has 529 passes in 83
suites (19 ignored); the selected RegExp Test262 corpus separately has 64
passes. Full ARM64/x64 qualification run 40604058677 has early cold reports from
all four runners. Both x64 cohorts pass all initial/repeated Boa decisions.
ARM64 copy 1 misses initial0.902731 and repeated0.903459; copy 2 passes
all initial decisions but misses repeated0.900208. All identical-executable
controls are within noise. The overall ARM64 speed condition is not met,
and same-source macro comparisons remain running. No threshold is relaxed.
The earlier UTF-16-key instruction profile is retained in profile-utf16-keys/;
its instrumentation results are not speed qualification.
