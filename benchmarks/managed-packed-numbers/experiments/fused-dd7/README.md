# Guarded numeric writes and direct indexed values

Runtime source: `dd7df4455bbcfcf8764464ff2185eb4650f1372a`.
Reference main: `c7c4f265c4002de1d90223eccaa64aaafdada52a`.

The preceding view trial failed hosted performance acceptance. Its complete
native reports, controls and matched-binary x64 diagnosis remain in `view-6fb`.
This is a distinct runtime change, not another measurement of that source.

Initialized mutable Number bindings now overwrite Number payloads directly,
after the existing TDZ/mutability guards. Other writes retain store_owned,
context checks, barriers and ordinary destruction. Value-only array reads now
perform the storage lookup and owner-checked load together, avoiding the
ElementRef value helper for descriptor-backed arrays. Reflective descriptor
operations still reconstruct every flag. Missing slots/accessors retain full
property lookup and the original receiver. Cache eligibility, GC schedule,
original workload bodies/counts/oracles and qualification gates are unchanged.

Local ARM64 native663 passed/21 ignored in88 suites; fmt, Clippy and selected
pinned Test262 array gates pass. The4 strict-provenance/symbolic-alignment Miri
integration tests pass/0 ignored with isolation enabled: independent descriptor
model, special payload/GC transitions, AST/Bytecode special numeric/object/
accessor reads, and numeric binding bits/TDZ/immutable/context/GC checks.
The same allocation probe still requests65024 bytes in7 requests for4096
numbers, validates8386560 and retains0 cells. Reference requests585216 bytes
in7 requests; this is allocation requests only, not peak/RSS or native timing.

Native CI40800358815 passes all4 jobs, including stable/Rust1.85/Clippy/fmt/C ABI.
The ARM64 before/after report validates all66 workers/1056 values and independently
recomputes1 faster/32 noise/0 slower decisions. Array ratios0.9916,0.9926,1.0000
are all noise; this does not establish an array speedup. Actual modified-Boa ARM64
and hosted two-machine comparison40800448415, plus full runtime security40800358738,
remain in progress. Each source
has separate evidence; earlier favorable or unfavorable results do not qualify
this one. No speed or merge acceptance is implied by these validation logs.

The completed x64 diagnostic40800451093 validates all6 original-input instrumented
runs. Whole-process Ir candidate/main ratios are0.96221 primitive-string-property,
0.97861 string-concat and0.97465 prop-mega. Some modeled events increase, including
conditional branch misses in all3 and L1 instruction misses in prop-mega. Retain
the complete counter vectors; these models are not actual hardware counters and
instrumented times are not native speed gates. Raw Callgrind files are lossless
gzip with both hashes in provenance. Binaries remain in the workflow artifact;
hashes/symbols/disassembly and raw profiles are retained here. Native binary
matching will be checked once the hosted acceptance reports are available.
