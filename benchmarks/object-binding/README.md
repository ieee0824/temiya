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
`node benchmarks/object-binding/verify-full.mjs` replays candidate and previous
qualified array91fb9f4 archives, including original known-pass gates, and
compares every outcome:33565 preceding passes preserved,0regressions,4354new.
The archived comparison uses the previous fully audited array implementation;
it is explicitly not the immediate parent6c65967, whose full capture and
separate comparison remain pending. All failures and timeouts are retained.
The original Symbol constructor cases pass both variants within this full
capture. Correctness evidence does not qualify native speed; both hosted
object-binding captures still fail the performance gate. CI keeps all prior
72 evidence commands and adds this full replay as command73.
