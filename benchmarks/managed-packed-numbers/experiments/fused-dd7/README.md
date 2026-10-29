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

Original11 ARM64 before/after and actual modified-Boa comparisons are pending.
Hosted two-machine comparison40800448415, native CI40800358815, full runtime
security40800358738 and x64 diagnostic40800451093 are in progress. Each source
has separate evidence; earlier favorable or unfavorable results do not qualify
this one. No speed or merge acceptance is implied by these validation logs.
