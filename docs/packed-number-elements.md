# Compact default numeric array elements

Issue #489 introduces a contiguous `Vec<f64>` representation for complete
numeric data descriptors with writable/enumerable/configurable all true.
Reads reconstruct those same descriptor flags; the f64 payload is copied
without arithmetic, preserving NaN, signed zero, infinities and subnormals.
Holes, sparse indices, accessors, nondefault attributes and mixed values expand
to ordinary descriptors before the existing validation/storage operations.
Expanded arrays keep that representation until an empty numeric refill.

The stored-descriptor representation uses the same typed handles and owner
checks as other managed arrays. Numeric-only slots have no managed edge and
need no per-slot tracing. Prototypes and named properties remain traced, as do
all descriptor-backed element representations. No collector interval or public
API changes are introduced. The separate ArraySetLength coercion work in #450
is unchanged; numeric length shrink and failed descriptor/shrink behavior retain
their existing checks.

The public integration tests preserve special numeric bits/default attributes,
exercise hole/sparse/mixed transitions and live/retired object edges across GC,
and compare operation sequences against an independent ordered descriptor
model. Existing JS/prototype/accessor/Proxy regressions also exercise the new
initial numeric representation. Native tests pass661/21 ignored on local ARM64;
Clippy passes, and both new integration tests pass strict-provenance/symbolic-
alignment Miri with isolation enabled. Selected pinned Test262 array-element,
array-stack and array-literal gates pass; skipped cases are not counted as passes.

`cargo run --locked --release --no-default-features --example array_allocation_probe`
measures Rust allocation requests for one clear/default-number refill after128
warmups using prebuilt keys and one existing public ArrayObject. With4096 slots,
the current main reference requests585216 bytes and the candidate65024 bytes,
both in7 requests, with sum8386560 and zero final retained cells. This is about
89% fewer requested bytes, not peak live bytes, RSS or a timing comparison.
The same probe source is used with unchanged reference library/manifests.

Performance qualification is pending. Keep the original eleven workload bodies,
counts, oracles,15 samples/3 alternating pairs,512MiB/180-second watchdogs and
the existing10%/MAD gate for candidate/main and actual pinned omoikane modified
Boa on ARM64 and hosted x64. Retain all decisions, regressions and controls.
The older pre-GC packed-number trial did not qualify an array speed improvement;
its unfavorable measurements remain in `docs/three-experiment-comparisons.md`.
