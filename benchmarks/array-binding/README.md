# Array binding implementation inputs (#162)

This branch builds on merged #161. Array binding now executes through
declarations, ordinary parameters,
catch and supported loop declarations. Boa speed qualification and current fixed-capture preservation
remain pending; this branch is not eligible for merge yet.

selection.json freezes all destructuring-binding metadata tests and every /dstr/
path from the complete pinned Test262 catalog before implementation. Object
binding, assignment, async/generator, eval and for-of dependencies remain in the
selection; outcomes must not remove them. selection-freeze.json records corpus,
catalog and per-file hashes. Skips, failures and timeouts are not passes.

Implementation must connect array binding to the shared IteratorRecord in
src/runtime/iterator.rs and cover declarations and function parameters. Required
observable behavior includes:

- BoundNames predeclaration for lexical bindings and non-simple parameters,
  preserving TDZ, left-to-right initialization and earlier parameter visibility.
- Elisions step without reading value; exhausted iterators do not advance again.
- Defaults run only for undefined and retain anonymous function/class naming.
- Rest drains into a real Array with own data properties; nested arrays recurse.
- Early normal completion closes once, including empty patterns. Abrupt defaults
  and nested initialization close unfinished outer iterators with the original
  JavaScript throw taking precedence. Step/done/value failures retain the shared
  record's done behavior. InternalError must never be masked by JavaScript.
- Ast and Auto public Engine tests verify side-effect order, receivers, getter
  failures, missing/non-callable methods, primitive values and reentrant GC.

Existing entry points to update are statement.rs binding_name, declaration
instantiation/evaluation, collect_var_names and applicable catch/for binding
paths, plus function.rs bind_parameters/bind_parameter. Keep identifier paths
fast. Object patterns are #163; assignment patterns are separate work. A selected
cross-feature test is retained even when its other dependency is unfinished.

Freeze performance workloads and validate their actual pinned Boa outputs before
runtime edits or candidate timing. Use original noise/deadline policy and all
prior control workloads; require each case to qualify independently on both
hosted machines. No timing reruns, CPU selection or changed oracle can turn an
adverse observation into a pass. Separate per-worktree Cargo target directories
are mandatory for frozen binaries.

The normative target is ECMAScript2026
[IteratorBindingInitialization](https://tc39.es/ecma262/2026/#sec-runtime-semantics-iteratorbindinginitialization)
and [BindingInitialization](https://tc39.es/ecma262/2026/#sec-runtime-semantics-bindinginitialization).
The frozen selection contains8806 files and17414 variants. Baseline and initial
candidate captures are retained below; focused integration tests are not Test262 passes.

The workload manifest and array-binding-benchmark-reference.mjs are materialized from the preimplementation frozen hashes; none of the six input bytes changed. The hosted workflow auto-includes this suite when include_controls is true and the manifest exists, preserving all earlier suites and the two-job policy. It retains actual Boa and identical-binary reports. The preimplementation baseline lacks array binding, so it is not labeled a successful before/after comparison. No hosted qualification is claimed yet.

`fixed-initial-ast-v1` preserves the full frozen 17,414-variant Ast captures
for the pre-implementation baseline c2bc478 and initial implementation 208ecd7.
The initial result is 5,291 pass, 7,691 fail and 4,432 timeout. The comparison
records 3,792 new passes and one previous pass becoming a timeout. A separate
same-binary/same-limit diagnostic is retained without replacing that timeout.
This is historical evidence; it does not qualify the later integrated source.
Replay both raw captures and their comparison with
`node benchmarks/array-binding/verify-fixed.mjs`.

`hosted-initial-integrated-v1` preserves run 41187179432 at 91fb9f4. Both
independent jobs record 0/3 significant wins for all six array-binding inputs
and for the two remaining Iterator inputs. All prior 46 workloads qualify on
both jobs. The candidate does not qualify for merging; all 72 raw reports and
adverse outcomes are retained. Replay with
`node benchmarks/array-binding/verify-hosted.mjs`.


`hosted-current-integrated-v1` retains run41207657318 at integrated source
6c65967. All72 reports independently replay with
`node benchmarks/array-binding/verify-hosted.mjs hosted-current-integrated-v1`.
All prior46 workloads qualify on both hosts. All6 array-binding cases record
0/3 significant wins on both; iterator wins3/3/3/3/0/0 both. Qualification
fails. Every adverse observation, CPU identity, noise and control remains.
The explicit beforeafter baseline is fd4ad7b; it is not the immediate parent.
No CPU filtering, workload/statistical/resource changes or timing repeats.


`full-current-integrated-v1` retains integrated source6c65967 run41207659226:
33565pass58059fail18skip8569timeout across all100211 variants,0missing.
The verifier now replays both retained capture pairs. For this integrated
capture it compares all outcomes against prior91fb9f4:33565passes preserved,
0regressions,0newpasses. Commit e53046e is the merge's first parent and has
identical compiled inputs to91fb9f4; the verifier checks this equivalence.
Both complete archives,policies,raw gate audits and outcome comparisons are
retained. These correctness results do not establish native Boa speed.

`fixed-array-data-properties-v1` retains historical runtime86679b1 fixed Ast/Auto captures, each5292pass7691fail4431timeout over17414variants, with zero missing results or collection errors. All1500 baseline Ast passes and1504 baseline Auto passes remain, adding3792/3788 passes. All5291 initial Ast passes and5292 initial Auto passes remain. The two new captures have identical outcomes. The original initial Auto collection failure (8unreported variants) and the separate missing-only recovery are preserved and independently reconstructed; the failed capture is never relabeled successful. Replay with `node benchmarks/array-binding/fixed-array-data-properties-v1/verify.mjs`. This historical result does not qualify the changed runtime9f84bc5.

Runtime9f84bc5 has separate two-job native and fixed Ast/Auto qualification in progress. Complete security independently replays19successful jobs:160exact Miri tests and932ASan tests. Its full Test262 run41234441785 independently audits100211variants with33565pass58059fail18skip8569timeout,0missing, and all33565previous array passes preserved. The fixed engine records its actual frozen sourcee9854f8; all83 recorded compiler input hashes match9f84bc5. No active capture is restarted or replaced by a newer observation.

`full-cache32-v1` retains the complete source9f84bc5 capture and its actual previous integrated source1090d929. Replay all four retained full-capture pairs with `node benchmarks/array-binding/verify-full.mjs`. Every older capture remains intact. `security-cache32-v1` preserves the complete source9f84bc5 security run41234443964, including all19job logs,16Miri plans/160executed exact tests and932ASan tests. Replay with `node benchmarks/array-binding/security-cache32-v1/verify.mjs benchmarks/array-binding/security-cache32-v1 9f84bc5c6416911df51d76cf10b936ad3c3e003f`. Neither result establishes native speed.
