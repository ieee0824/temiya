# Object binding preparation (#163)

Initial runtime implementation is under correctness verification; no merge or
speed qualification is claimed. Commit 75d6b2a freezes the complete
17,415-variant selection and six authored workloads before candidate timing.
The selection retains array, assignment, async and generator dependencies.
Every workload executes 10,000 iterations with expected sum 50,005,000.
The actual pinned Boa binary validates each oracle twice. This provides no
candidate speed qualification.

Run `node benchmarks/object-binding/verify-workloads.mjs` to verify the frozen
inputs, selection, reference identity and saved Boa output hashes.

Coverage planned: declarations and parameters, nested patterns, computed key
conversion order, defaults and TDZ, rest key exclusions, symbols, enumerability,
getter order/abrupt completions, null/undefined and primitive sources. Merge
after #161 and #162, with independent correctness and unchanged speed gates.

`initial-profiles-v1` retains the first six Callgrind captures at 1076d17,
including all 82 build input hashes, worker outputs, raw compressed instruction
profiles and logs. Every output validates the original expected value twice.
Replay with `node benchmarks/object-binding/verify-profiles.mjs`. These are
instrumented instruction diagnostics and do not qualify native Boa performance.

`hosted-initial-v1` preserves run 41188431466 at 1076d17. Both independent
jobs record 0/3 significant wins for all six object-binding workloads. Every
original/control observation is retained; the source does not qualify for
merging. Independently replay all 76 reports with
`node benchmarks/object-binding/verify-hosted.mjs`.


`hosted-current-integrated-v1` retains both hosted jobs of run41207721402
at source010ac60 (array parent6c65967). All76 reports independently replay
with `node benchmarks/object-binding/verify-hosted.mjs hosted-current-integrated-v1`.
Prior46 workloads qualify both hosts, but all6 array-binding cases are0/3
both. Object-binding cases are0/3 except getter-order1/3 on copy2. Iterator
wins are3/3/3/3/0/0 both. Qualification fails; adverse timings, CPU identities,
noise and controls are preserved without filtering or unchanged-source repeats.


`full-current-integrated-v1` retains all8 shards of run41207723463 at010ac60:
37919pass53705fail18skip8569timeout across100211 variants,0missing.
`node benchmarks/object-binding/verify-full.mjs` replays the candidate,previous
qualified array91fb9f4 and immediate parent6c65967 full archives, including
all original known-pass gates. Separate comparisons against both complete
100211-variant captures preserve33565passes,with0regressions and4354newpasses
in each. The immediate-parent capture and comparison are now retained. All failures and timeouts are retained.
The original Symbol constructor cases pass both variants within this full
capture. Correctness evidence does not qualify native speed; both hosted
object-binding captures still fail the performance gate. CI keeps all prior
72 evidence commands and adds this full replay as command73.

Local correctness on source `5ff8f9a6f99736fde85e966564d6468f813ce68b`
is retained in `inline-parent-correctness-v1`: 947 all-target tests, one doc
test, Clippy, i686 check, and all 11 object-binding integration tests under
strict Miri passed. Replay with
`node benchmarks/object-binding/inline-parent-correctness-v1/verify.mjs`.
These logs do not establish native speed or full Test262 qualification.

Source `fe67ff56a766d834bc70c4e8219f4fad47b06723` is retained in
`hosted-inline-parent-v1`: both unfiltered AMD EPYC 9V45 jobs fail speed
qualification. Object wins are 0/0/0/0/3/0 and 0/0/0/0/1/0; preceding array
rest and TDZ-close each record 0/3 on both jobs. All 76 reports and adverse
observations remain. Complete source security passes all 19 jobs, with 163
strict Miri tests and 947 ASan passes, in `security-inline-parent-v1`.
Safety success does not qualify native speed.

`allocation-diagnostics-v1` retains all six unchanged workloads at fe67,
15a5671 and 3dad119, including raw compressed profiles, outputs and build
input hashes. The direct-get change reduces instructions in every case
(about 8.9% for declarations and 8.1% for parameters). Sharing empty optional
object state increases instructions in all six cases relative to direct-get;
that prototype is withdrawn. The lifecycle regression test remains. Replay
with `node benchmarks/object-binding/allocation-diagnostics-v1/verify.mjs`.
These instrumented measurements do not establish native Boa speed.

`full-inline-parent-v1` retains all eight shards of run 41245246099 at
fe67ff5: 37,919 pass, 53,705 fail, 18 skip and 8,569 timeout across
100,211 variants, with none missing. Its verifier replays this capture and
the retained array7a154b1 and previous-object010ac60 captures, then compares
every variant: all 33,565 array passes and 37,919 previous object passes
remain, with zero regressions. This historical source still fails native
speed qualification; these results do not transfer to changed runtime.
Run `node benchmarks/object-binding/full-inline-parent-v1/verify.mjs`.

The fourth allocation diagnostic, `name-visitor` (3d08637), validates the
complete binding pattern before visiting names directly. It removes the
temporary name Vec while retaining callback failure order. All949 tests
(26 ignored), Clippy and release build pass. Five instruction profiles
improve relative to direct-get; parameters rise by about0.063%. This is
retained as mixed evidence and is not a native performance qualification.

Fresh array initialization at aa77879 bypasses the generic DefineOwnProperty
path only for the unpublished object created by ArrayObject. The immutable
numeric length has the same writable/non-enumerable/non-configurable descriptor.
All949 tests (26 ignored), focused43 tests (one ignored), Clippy and release
build pass. `fresh-array-diagnostics-v1` retains every before/after array
profile: declaration instructions drop5.72%, parameters4.47%, rest5.00%;
the other three change by less than0.004%. Allocation diagnostics also retain
all six object profiles at this source. Neither establishes native Boa speed.
Replay with `node benchmarks/object-binding/fresh-array-diagnostics-v1/verify.mjs`.

Single catch bindings at6587184 remain inline until record extension, avoiding
the initial binding Vec. Name hints, mutability/non-deletability, closure cells
and managed edges remain checked. `single-catch-correctness-v1` retains951
all-target passes (26 ignored), Clippy/release and two exact strict Miri tests,
including GC/promotion and public Ast/Auto closure mutation. All six instruction
profiles are retained under allocation diagnostics: abrupt-coercion drops2.26%
relative to aa77879, while the other five rise0.27–0.42%. This is mixed
diagnostic evidence, not native performance or complete security qualification.
Replay with `node benchmarks/object-binding/single-catch-correctness-v1/verify.mjs`.

`security-direct-get-v1` retains all19 successful jobs from run41254034829
at1807734. Independent replay verifies163 unique exact strict Miri tests
and948 ASan passes. This is preceding-source safety evidence; it does not
qualify name-visitor, fresh-array or single-catch changes, nor native speed.

`full-direct-get-v1` retains all eight shards from run41254032628 at1807734:
100211 variants,37919pass53705fail18skip8569timeout,0missing. Its verifier
replays candidate, fe67 previous object and7a154b1 array archives, preserving
all37919 and33565 preceding passes respectively, with zero regressions.
This source precedes later optimizations and cannot qualify their behavior.
Replay with `node benchmarks/object-binding/full-direct-get-v1/verify.mjs`.

The single-catch local correctness capture additionally retains45 MSRV
integration passes (one ignored) and a successful i686 all-target check.
