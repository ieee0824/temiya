# Compact default numeric array elements

Issue #489 stores contiguous, complete numeric data descriptors in `Vec<f64>`
when writable/enumerable/configurable are all true. Payloads are copied without
arithmetic, retaining NaN bits, signed zero, infinities and subnormals. Reflective
reads reconstruct every default flag. Holes, sparse indices, accessors,
nondefault attributes and mixed values expand to ordinary descriptors before
existing validation/storage operations. Expanded arrays retain that form until
an empty numeric refill; writes do not repeatedly scan arrays to repack them.

Value-only indexed reads combine storage lookup with the ordinary owner-checked
value load. Missing entries and accessors use full property lookup with the
original receiver. Numeric slots contain no managed edges; prototypes, named
properties and every descriptor-backed element remain strongly traced.

The x64 regression investigation also identified unnecessary general binding
write work. An initialized mutable Number binding now accepts another Number
by copying its payload after TDZ/mutability checks. Other writes retain
`StoredValue::store_owned`, context checks, barriers and old-payload destruction.
No public API, cache eligibility, collector interval or execution budget changes
are introduced. ArraySetLength double coercion #450 remains separate; numeric
length shrink and failed descriptor/shrink behavior retain their existing checks.

The qualified runtime source is `dd7df4455bbcfcf8764464ff2185eb4650f1372a`, compared
with main `c7c4f265c4002de1d90223eccaa64aaafdada52a`. Full evidence is in
`benchmarks/managed-packed-numbers/experiments/fused-dd7`.

Local ARM64 native tests pass663/21 ignored in88 suites; fmt and Clippy pass.
[CI40800358815](https://github.com/ieee0824/temiya/actions/runs/40800358815) passes
stable/Rust1.85, formatting/Clippy and C ABI. All4 public integration tests pass
strict-provenance/symbolic-alignment Miri with isolation enabled. Full runtime
[security40800358738](https://github.com/ieee0824/temiya/actions/runs/40800358738)
passes119 distinct exact library Miri tests,0 ignored, the complete16-shard
partition and all19 jobs, including ASan and dependency audit. Source/status/full
logs and independent audits are retained; older-source results are not relabeled.

The special-number test checks every payload and default flag through holes,
sparse storage, descriptor restoration and GC, then directly overwrites numeric
storage with a managed child and verifies retention/release. An independent
ordered descriptor model compares1024 native operations (48 under Miri), keys,
length, flags, values and failed operations. AST/Bytecode differential checks
cover special numbers, object-valued indexed reads and own/inherited accessor
receivers. Binding tests check exact bits, TDZ, immutable strict/sloppy writes,
rejected foreign-context objects, child retention/release and type transitions.
Fixed Test262 array-elements, array-stack and array-literal wrappers execute the
pinned corpus revision and pass. Ignored-by-default selected wrappers are run
explicitly; this is selected regression coverage, not the entire Test262 suite.

The unchanged original-eleven performance protocol uses15 timed samples plus a
checked warmup,3 alternating pairs,512MiB/180-second watchdogs and the existing
`max(10%, 3*(relative MAD A + relative MAD B))` gate. Setup/startup/warmup are
excluded from timed samples; each timed pass parses and evaluates the same
invocation. No body, count, oracle, pair or unfavorable observation is discarded.
The reference is the actual modified Boa and public sandboxed embedding in
omoikane at007556adccb8fb4329236932ca8175f8e8d119d2, with original features/locks
and no JIT. Both workers use Rust1.99.1 release builds; Temiya has no default
features. The real embedding's cooperative limits and Temiya's lack of an
equivalent cooperative budget remain explicitly recorded in the native reports.

| Observed cohort | Candidate/main | Candidate/actual modified Boa | Controls |
| --- | --- | --- | --- |
| ARM64 guestCPU0 |1 faster/32 noise/0 slower|33 faster beyond gate|not measured|
| Hosted AMD EPYC7763 |3 faster/30 noise/0 slower|33 faster beyond gate|baseline/Boa33 faster; same binary33 noise|
| Hosted Intel Xeon Platinum8370C |33 noise/0 slower|33 faster beyond gate|baseline/Boa33 faster; same binary33 noise|

All10 reports independently validate66 workers/1056 values each, original inputs,
statistics, source/compiler/feature/lock/binary identities and controls. Hosted
machines were independently assigned without a model filter. The diagnostic
rebuilds exactly match both native hosted binaries. All9 array before/after
pairs are within their gates: no array timing improvement is established.
Qualification applies to these workloads and observed environments, not arbitrary
JavaScript or SpiderMonkey's JIT. ARM affinity and its physical-host limitations
are recorded; agent affinity is restored after both terminal measurements.

`cargo run --locked --release --no-default-features --example array_allocation_probe`
measures one clear/default-number refill after128 warmups with prebuilt keys and
one public ArrayObject. For4096 numbers, reference requests585216 bytes and
candidate65024 bytes (7 requests each), validates sum8386560 and retains0 cells.
This is about89% fewer requested bytes, not peak live bytes, RSS or a timing win.
The identical probe was temporarily added to the unchanged reference library;
it was removed before freezing the clean baseline timing worker.

Earlier pre-GC, Cow and small-view trials retain their unfavorable reports and
controls. See `docs/three-experiment-comparisons.md`, `experiments/cow-25e` and
`experiments/view-6fb`. The view's hosted4/5 regressions and one Boa miss were not
accepted despite passing safety checks. Matched-binary x64 profiling localized
extra general binding-write and indexed-value helper work; the follow-up was
then separately qualified. Raw instruction profiles/disassembly are retained
with lossless gzip and both hashes. Some modeled branch/cache counters increase;
model counters and instrumented times never replace native speed gates. The
upstream audit at main49715ad confirms identical vendored engine/Cargo objects,
while app JS/DOM changed; no timing of that newer complete embedding is claimed.

| #489 requirement | Evidence |
| --- | --- |
| Special numeric payloads and default flags | Exact-bit public test, GC and representation round trips; four integration Miri passes. |
| Hole/prototype/descriptor/length/mixed/sparse transitions | Public array tests, AST/Bytecode receiver tests, independent descriptor model and GC checks. |
| Independent model and pinned Test262 |1024 native/48 Miri model operations and explicitly executed fixed array wrappers. |
| Original-eleven ARM64/hosted comparisons and adverse-result investigation | All10 reports, complete controls/identities/statistical audits, both older failed trials and matched-binary diagnosis. |
| Stable/MSRV/quality/security/results and integration | CI40800358815, security40800358738, retained logs/audits and this document; final integration/merge state is [PR #524](https://github.com/ieee0824/temiya/pull/524). |
