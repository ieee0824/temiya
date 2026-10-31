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
