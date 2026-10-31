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
merged or that #159 has any runtime support yet.

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
