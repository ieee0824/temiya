# Private instance field qualification inputs

Before implementing #159, selection.json freezes the union of all parsed files
with class-fields-private, class-fields-private-in or
nonextensible-applies-to-private metadata in the pinned complete reference
catalog: 1138 files and2256 variants. No execution outcomes are used to select
files. selection-freeze.json records every source hash, the catalog hash and the
corpus revision. Dependencies on private methods, static elements, eval and other
features remain selected, even though #160 separately covers private methods,
accessors and static private elements.

The preimplementation runtime is612d759 (the pending #158 implementation), with
its immutable binary and build identities in
/tmp/temiya-158-constructor-assignments-v1. This is not a claim that #158 has
merged. Initial #159 runtime support is under validation; no merge qualification
or complete conformance is claimed.

The project targets ECMAScript2026. Implementation must follow
[PrivateFieldAdd](https://tc39.es/ecma262/2026/#sec-privatefieldadd),
[PrivateGet](https://tc39.es/ecma262/2026/#sec-privateget),
[PrivateSet](https://tc39.es/ecma262/2026/#sec-privateset) and
[private in evaluation](https://tc39.es/ecma262/2026/#sec-relational-operators-runtime-semantics-evaluation).
Private names need class-evaluation identity and lexical resolution; private
values must have traced storage separate from ordinary properties. Receivers,
duplicate initialization, nested scopes, update/compound assignment and calls
need actual Engine tests in Ast and Auto, including exception/side-effect order.

The pinned corpus contains historical nonextensible-applies-to-private tests
whose expected rejection differs from the2026 PrivateFieldAdd algorithm.
Retain their actual outcomes and record this discrepancy; do not make private
field storage obey ordinary property extensibility merely to pass those tests.
No skip, failure, missing result or unmeasured benchmark counts as a pass.

workloads-v1 freezes five10000-iteration inputs with expected50005000 before
the first candidate timing. They cover construction/read, mutation, positive
and negative brands, lexical capture and a wrong receiver on the final iteration.
All five passed actual pinned Boa twice (warmup and sample); oracle probes
establish outputs only, not performance. workload-freeze.json records hashes.

initial-v1 retains both complete before/after captures. In each backend, all752
preceding passes remain and655 cases newly pass:1407pass,697fail,152timeout.
All2256 Ast/Auto outcomes match. The source7482311 has855 passing native tests
and26 ignored. Failure diagnostics include deferred private elements, eval,
async/generator execution, destructuring assignment and historical expectations;
all failures and timeouts remain in the evidence and are not waived.

private-expression-v1 records sourcee6722cb: strict zero-parameter methods whose
body only returns a private read/update use the shared evaluator with their
captured class environment, without creating an unobservable activation.
Numeric coercion, BigInt, thrown identity, wrong brands and per-class lexical
identity retain Ast/Auto tests. All856 native tests and Clippy pass. Callgrind
records195821712 instructions before and130138634 after on private-update;
profiled wall times are not speed qualification. New-source original outcomes
and hosted performance remain pending.

Run verify-initial.mjs to reconstruct all four retained before/after captures,
including752 preserved passes,655 new passes and all remaining failures/timeouts.
verify-private-expression.mjs checks both source/binary identities, native tests
and the complete Callgrind instruction totals. These integrity checks do not
establish a Boa speed pass. Both verifiers run in CI.

Current qualification is incomplete. full-default-private-v1 preserves all27200
preceding-main passes and adds655 across100211 variants. The sourcee3eb587
two-host run41158818113 is retained in hosted-default-private-v1: both machines
fail private-invalid-receiver (0/3 significant wins), and copy2 also fails
static-abrupt (1/3). Successful capture integrity is not a speed pass.

Sourcea65fa12 binds private-expression identities per closure and avoids unused
closure roots in prepared base constructors. bound-private-diagnostic-v1 retains
the unchanged private-invalid-receiver input: Callgrind instructions fall from
536133532 to512342212. All861 native tests pass, including the added reentrant
GC environment regression; Clippy, MSRV and stable i686 checks pass. Run
verify-private-expression.mjs bound-private-diagnostic-v1 to replay this
diagnostic. Its new hosted/full-corpus/safety qualification is pending.

full-bound-private-v1 replays sourcea65fa12 across52133files100211variants:
27855pass63525fail18skip8813timeout,0 missing. All27855 previous-candidate
passes remain; all27200 previous-main passes remain with655 additions. This
full-corpus gate does not replace the separate two-host speed and safety gates.

full-bound-constructor-v1 retains source072f6315 full-corpus results after inline
private storage and bound constructor bindings:27855pass,63525fail,18skip,
8813timeout,0missing. All previous candidate passes remain, and all27200 main
passes remain with655 additions. The eight gates independently replay; this
correctness result does not establish the separate pending two-host speed gate.

hosted-bound-constructor-v1 retains run41168380212, source072f6315. Copy1
EPYC7763 qualifies all40 workloads. Copy2 EPYC9V74 qualifies38; static-abrupt
and private-invalid-receiver both have0/3 significant wins. All56 reports
independently replay. Neither a successful job nor one passing host qualifies
this source for merge. The original inputs and noise policy remain unchanged.

local-bindings-diagnostic-v1 and identifier-values-diagnostic-v1 retain the
unchanged private-invalid input across9ed3bc7/31fa20e/03d3724:490862016 to
481149396 to431811114 instructions. Pure local value reads fuse name resolution
and GetValue; reference contexts keep their original paths. Source03d3724 has
862 native tests including1doctest,0fail26ignored and Clippy passed. The
interrupted permission run and its recovery are retained separately. These
instruction diagnostics do not establish native Boa speed qualification.
