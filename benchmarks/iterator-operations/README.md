# Synchronous iterator abstract operations

Before #161 implementation, selection.json freezes all1947 files and3880
variants in the metadata/path union recorded in selection-freeze.json. It is
selected from the complete pinned52133-file catalog without outcome filtering.
The immutable baseline is runtime622bc975 (pending #160), whose80 compiled
inputs are identical to this branch's starting point. This is not a claim that
#159 or #160 has merged or qualified for speed.

The implementation must share synchronous iterator acquisition, cached next
method calls, done/value reads and completion-aware closing. Existing array
literal and argument spread consumers must enter these operations through the
public Engine Ast/Auto paths. Keep read/call order, non-object validation, thrown
value identity, receiver identity and reentrant GC behavior covered.

The ECMAScript2026 operation definitions are the normative target:
https://tc39.es/ecma262/2026/multipage/abstract-operations.html#sec-getiterator
https://tc39.es/ecma262/2026/multipage/abstract-operations.html#sec-iteratorclose

For-of integration remains #171, which explicitly depends on #161. Array binding
patterns remain #162; asynchronous protocol support and later builtins retain
their own issues. Their tests remain in this complete metadata selection, and
failures/timeouts cannot count as passes. No runtime implementation or new speed
qualification is claimed by this input freeze. Performance inputs must also be
frozen and checked with actual pinned Boa before first candidate timing.

The initial shared-record runtime is implemented locally. All878 native tests
pass (0failed26ignored), with Clippy, MSRV and stable i686 all-target checks.
Six10000-iteration workloads were frozen before implementation; actual pinned
Boa and the immutable baseline each validated every warmup/sample oracle. These
checks establish outputs only; no new native speed pass is claimed. The
1947-file/3880-variant baseline remains in progress.

validation-v1 retains the dedicated-target sourcec2bc478 build:878 native
passes,0 failures,26 ignored, Clippy/MSRV/i686 logs and36 successful output
validations across six cases and three engines/builds. verify-validation.mjs
checks source/input identities and preserves the rejected shared-target source
probe as unqualified. Use a separate CARGO_TARGET_DIR per worktree, then freeze
the verified executables before starting another build.

The comparison/audit tools accept --iterators. The hosted workflow records the
frozen suite when include_controls is true and its manifest exists. It retains
before/after, actual Boa, baseline/Boa and identical-binary controls with the
existing15sample/3pair policy; previous suites remain included. These wiring
and output checks do not establish native speed qualification.

full-initial-v1 retains the full source7eba69e capture from run41169970673:
29701pass61923fail18skip8569timeout,0missing across100211 variants and52133
files. All29701 parent passes remain, with0newpasses and0regressions. The8
known-pass gates independently replay via verify-full.mjs in CI. These results
do not imply complete conformance or a passing two-host speed qualification.

fixed-initial-v1 retains all four fixed before/after Ast/Auto captures and an
independent replay in verify-fixed.mjs. Each candidate backend records235pass,
1983fail,1662timeout across3880 variants; their outcomes agree exactly. Baseline
Ast passes235 and Auto passes194; no prior pass regresses. The baseline Auto
producer left8 variants wholly unreported after one failed CLI batch. Recovery
used the identical binary/host/limits only for those8, preserving all3872
reported outcomes, failed raw attempt and initial history. All130 baseline
Auto timeout differences remain visible (41 now pass,89 now fail); they are
not claimed as new semantic coverage. No timeout, failure or skip is a pass.

hosted-initial-v1 retains all68 independently replayed reports from
run41169970601, source7eba69e. All46 preceding workload gates pass on both
hosts. Iterator wins in frozen order are0/0/2/3/0/0 on EPYC9V45 and
0/0/0/1/0/0 on Xeon8573C. Call spread is roughly11x/21x Boa time,
done/value getters roughly4x, and invalid-next-result roughly2x. These
adverse results do not qualify the source for merge. No input, limit, CPU
selection or significance threshold has been changed.

indexed-arguments-diagnostic-v1 retains source74ff04d: call-spread instructions
5286694951 to343139030,882 tests including1doctest passed. Fresh indexed
argument storage and fully checked lookup hints avoid quadratic property scans.
definition-cache-diagnostic-v1 retains source09766bb: getter-order2990744792
to1272405193 and invalid-next1624065902 to1148010885 instructions,884 tests
including1doctest passed. Verified bodies retain syntax/code-only caches across
calls; Realm, environments, function objects and private identities bind anew.
Cross-Realm code reuse, escaping captures, code equality and heap release have
public integration coverage. verify-profiles.mjs checks identities, raw profiles,
native logs and all6 output oracles. These are instruction diagnostics only.

`full-definition-cache-v1` retains run 41176606769 at source 09766bb. All eight full-corpus gates replay independently; all 29,701 preceding passes remain, with no new passes or regressions. Existing failures and timeouts remain recorded. This capture does not establish speed qualification.

`hosted-definition-cache-v1` retains all 68 reports from run 41176605076. Existing 46 workloads qualify on both jobs; five of six Iterator workloads still fail the speed requirement. Integrity replay preserves the adverse timings and does not qualify this implementation for merge.

The fourth diagnostic point (0269ef9 environment-root cache) is rejected and reverted: getter instructions improve but invalid-result instructions increase. Raw profiles and the successful bounded-root test remain evidence, not qualification.

`key-and-result-diagnostic-v1` retains 24 source-specific Callgrind points, including rejected experiments. `verify-key-profiles.mjs` checks raw counts, output oracles and the exact compiled-input inventory for each historical source (80, 81 or 82 inputs). Later points cover function frames, activation roots, short callbacks, owned function sites, weak environment hints and literal accessor initialization. These diagnostics do not satisfy the native Boa speed gate.

`fixed-definition-cache-v1` retains historical09766bb fixed Ast/Auto captures:235pass1983fail1662timeout each. Every outcome agrees with c2bc478; no regressions or missing records. These captures do not qualify the later key/result/native-step changes.

Native-error (c247cf3) and native-step (83dc7b0) diagnostics are also retained. Native-step validates all six frozen outputs twice; array spread records219,270,403 instructions and call spread221,612,852. Custom getter/invalid-result cases remain recorded, and hosted speed qualification is pending.

`hosted-native-step-v1` retains run 41181490909 at 1979ac3. All previous
46 workloads qualify on both independent jobs. Four iterator workloads qualify;
`done-value-order` and `invalid-next-result` each record 0/3 significant wins
on both jobs. This capture does not meet the speed requirement. Replay its
68 reports with `node benchmarks/iterator-operations/verify-hosted.mjs hosted-native-step-v1`.

`full-native-step-v1` retains all eight shards of run 41181492659 at
1979ac3: 29,701 pass, 61,923 fail, 18 skip and 8,569 timeout. All 29,701
preceding candidate passes remain passes, with no new passes or regressions.
Replay with `node benchmarks/iterator-operations/verify-full.mjs full-native-step-v1`.

The `empty-frame` node in `key-and-result-diagnostic-v1` retains all six
4a4c103 Callgrind profiles and source hashes, including the new 81st Rust
input. Every worker output validates the unchanged expected value twice.
These instrumented instruction counts do not establish native speed versus Boa.

`full-empty-frame-v1` retains all eight shards of run 41186658237 at
4a4c103: 29,701 pass, 61,923 fail, 18 skip and 8,569 timeout across 100,211
variants. The preceding native-step candidate's 29,701 passes remain passes,
with no new passes or regressions. The verifier reconstructs this comparison
from both archives. This is correctness evidence, not native speed qualification.
Replay with `node benchmarks/iterator-operations/verify-full.mjs full-empty-frame-v1`.

`hosted-empty-frame-v1` retains run 41186655959 at 4a4c103. Both jobs record
0/3 significant wins for done/value order and invalid next results. Copy 2 also
records 1/3 for captured-next and for the preceding derived-definition control.
All 68 reports independently replay; the source does not meet the speed gate.
Replay with `node benchmarks/iterator-operations/verify-hosted.mjs hosted-empty-frame-v1`.

The `error-strings`, `activation-fingerprints`, and `inline-stacks` nodes in
`key-and-result-diagnostic-v1` preserve all original 48185f4, 5852248 and 17905b8
profiles with their build-input hashes and twice-validated worker outputs.
The combined inline-stack experiment increases instructions for five of six
inputs (up to about 15.7%); its operand representation was reverted in d9906db while the separate auxiliary
stack storage was retained. These remain diagnostics,
not native Boa qualification. Replay with `verify-key-profiles.mjs`.

`fixed-empty-frame-v1` retains complete fixed Ast/Auto outcomes at f678ea7,
a metadata-only build of runtime 4a4c103. Both backends record 235 pass, 1,983
fail and 1,662 timeout across 3,880 variants, with every outcome matching the
preceding 09766bba source. The original baseline collection failure and its
missing-only recovery remain archived. `verify-fixed.mjs` checks the raw reports
and runtime/build source equivalence. This does not qualify later runtime edits.

`full-inline-auxiliary-v1` retains run 41192023972 at d9906db. All eight
complete-corpus known-pass gates replay independently, and all 29,701 preceding
4a4c103 passes remain passes, with zero new passes or regressions. Failures,
skips and timeouts remain recorded. Replay with
`node benchmarks/iterator-operations/verify-full.mjs full-inline-auxiliary-v1`.
This is correctness evidence and does not establish native speed qualification.


`full-literal-accessors-v1` retains run 41204131637 at 702ee76 (compiled
runtime 0e9c642). All 100,211 variants are independently audited. Comparison
against `full-weak-environments-v1` preserves all 29,701 preceding passes,
with zero regressions or new passes. The 61,923 failures, 18 skips and 8,569
timeouts remain recorded. Replay both archive integrity and the parent
comparison with `node benchmarks/iterator-operations/verify-full.mjs full-literal-accessors-v1`.

`hosted-weak-environments-v1` retains all 68 reports from run 41200250693
at fd4ad7b (compiled runtime 37a6d37). In workload order from the frozen manifest,
iterator significant wins are 3/3/3/3/0/0 on copy 1 and 3/3/3/3/2/3 on copy 2.
All preceding 46 workloads qualify on both hosts. Because two iterator cases
fail on copy 1, this source fails the unchanged two-host speed gate. Replay with
`node benchmarks/iterator-operations/verify-hosted.mjs hosted-weak-environments-v1`.
This capture cannot qualify the later literal accessor runtime.

`security-owned-sites-timeout-v1` preserves the original 60-minute Miri
shard timeout in run 41198356374. Eight selected tests completed, the ninth
was interrupted and the tenth did not start. Neither incomplete test counts
as a pass. Commit d8c8dca increases the Miri job budget to 90 minutes without
changing the 16 shards, test selection or strict interpreter flags. The timeout
record and focused Miri diagnostics do not replace a completed full security run.


The `weak-objects` diagnostic records rejected runtime c6e8fd8 and its
revert 2b13f1f. Non-owning stored-object facade hints pass 912 all-target tests,
Clippy, MSRV, i686, doctest and a focused strict Miri lifetime/context test.
Four instruction profiles improve by about 0.3–3.1%, but done/value order
increases by 0.13% and invalid next results by 0.37%. Both are the remaining
unqualified iterator cases, so the change was reverted without a new native
measurement. All six unchanged worker outputs validate twice; raw profiles,
source hashes and validation logs remain replayable.


The `hot-sites` node retains runtime fdecf72. Two recent owned function sites
avoid repeated hash lookups; borrowed syntax keeps structural comparison and
no JS activation is retained. Done/value instructions decline 1.16% and invalid
result instructions 1.30%; the other four cases are nearly unchanged, including
a 0.0052% captured-next increase. All six oracles validate twice. All-target
tests912, Clippy, MSRV, i686, doctest and three focused strict Miri tests pass.
These are instruction diagnostics; current-source native and full correctness
qualification remain required before merge.


`fixed-literal-accessors-v1` retains completed runtime0e9c642 fixed captures:
Ast and Auto each235 pass,1983 fail,1662 timeout across3880 variants, with
zero missing outcomes or collection errors. Every outcome matches the prior
d9906db capture and the other backend. The original baseline collection
failure and missing-only recovery remain archived. Replay all retained
fixed capture pairs with `node benchmarks/iterator-operations/verify-fixed.mjs`.
These results do not qualify the later fdecf72 runtime.


`hosted-literal-accessors-v1` retains both jobs of run41204129150. All68
reports independently replay. Iterator wins3/3/2/3/0/0 and3/3/3/3/1/1 fail
the unchanged two-host gate; copy2 derived-definition also qualifies only1/3
pairs. CPU models, noise, controls and adverse observations remain recorded.

The `reference-updates` diagnostic retains runtime77e9c10, which reuses
the VM checked numeric-binding write for resolved reference updates. BigInt,
coercion, property and unresolvable references retain the complete operation.
Relative to fdecf72, instructions decline4.73% for done/value,1.76% for invalid
results and4.04% for captured-next; other cases are nearly unchanged, including
a0.0020% array increase. All6 expected outputs validate twice. All-target
tests912, Clippy, MSRV, i686 and doctest pass. The focused strict Miri
Ast/VM numeric-update integration test also passes (one test,194.48seconds);
its raw log, flags and test-source hash are retained. This diagnostic
does not qualify native speed or replace current-source full/fixed/security gates.


The `direct-region` diagnostic retains runtime9835c41. The activation guard
owns the bounded root region through Runtime state, avoiding weak promotion
and a second borrow at every object load. Teardown removes the region before
dropping facades, breaking temporary ownership cycles; cache capacity, eviction
and pre-collection clearing are unchanged. A new last-external-owner test
checks Runtime release. All913 tests,Clippy,MSRV,i686 and doctest pass.
All6 instruction counts improve by0.74–2.08% relative to77e9c10; unchanged
expected outputs validate twice. The focused strict Miri6-test selection
also passes (220.36seconds), covering reentrant GC, bounded retention, unwind
and final Runtime release; its complete raw log and strict flags are retained. Current full correctness, security
and two-host native qualification remain required.


The `catch-records` node retains runtime71009f3. Fresh catch records contain
their initialized parameter before publication, using checked stored values
and incoming-record tracing at allocation. Catch without a parameter uses the
existing block environment logic. All914 tests,Clippy,MSRV,i686,doctest and a
focused strict Miri test pass; the latter checks allocation-time collection,
foreign-heap rejection, mutable/nondeletable bindings and eventual graph release.
Invalid-result instructions decline1.06%; other5 cases increase0.0006–0.0062%,
all retained. Every unchanged expected value validates twice. These diagnostics
do not qualify native speed or replace current full/fixed/security gates.


`hosted-catch-records-build-failure-v1` retains copy1 of run41211990414,
which stopped before Boa compilation because the space guard found10.9GiB
free against its unchanged14GiB requirement. No frozen workload was measured;
40+2 mode probes are not qualification evidence. The original log and all6
artifact files remain hash-checked by verify-hosted.mjs. The workflow now
removes unused Temiya intermediates and unused hosted-runner SDKs before the
reference build, preserving worker hashes and checking worker execution again.
The guard,compiler flags,fixtures,oracles,sample counts,statistical thresholds
and runtime watchdog/RSS limits are unchanged. Copy2 observations remain intact.


`fixed-hot-sites-v1` and `fixed-direct-region-v1` retain completed fdecf72
and9835c41 Ast/Auto captures. Each backend records235pass1983fail1662timeout
across3880 variants,with0missing/collection errors and every outcome matching
preceding0e9c642. The original baseline collection failure and missing-only
recovery remain included. verify-fixed.mjs now replays all7 historical pairs;
these observations do not qualify the later71009f3 runtime or native speed.

`hosted-hot-sites-v1` retains both jobs of41208962674 atbb0d326,compiled
runtimefdecf72. All68 reports independently replay. Prior46 workloads qualify
both hosts;iterator wins3/3/3/3/0/0 both,so qualification fails. Every adverse
observation,CPU identity,noise estimate and control remains without filtering
or repeated timing.

`full-hot-sites-v1` retains all8 shards of41208965193 atbb0d326. All100211
variants independently replay:29701pass61923fail18skip8569timeout. Comparison
with full-literal-accessors-v1 preserves all29701 preceding passes,with0
regressions/newpasses. Replay with verify-full.mjs full-hot-sites-v1. Full and
fixed correctness records do not substitute for current-source native/security
qualification.
