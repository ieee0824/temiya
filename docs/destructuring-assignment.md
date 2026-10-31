# Destructuring assignment

The interpreter evaluates array and object assignment patterns through the
existing iterator and property-copy machinery. Identifier and member targets
produce References; writes use PutValue rather than declaration initialization.
The assignment expression returns its original right-hand value. Supported
for-in/of expression targets use the same path.

Target references are evaluated before the corresponding iterator step or
property get, followed by default evaluation and the write. Earlier writes
remain observable if a later target throws. IteratorClose preserves the original
throw completion. Object rest resolves its target before copying properties.

Public Engine tests exercise Ast and Auto in
`tests/destructuring_assignment.rs`. The implementation is pending fixed
Test262, safety, toolchain, and native Boa performance qualification. It does
not establish complete for-of or generator support. Issues #162 and #163 must
land before #164.

Specification: [ECMA-262 destructuring assignment](https://tc39.es/ecma262/2026/multipage/ecmascript-language-expressions.html#sec-destructuring-assignment).

Local correctness evidence is retained in
`benchmarks/destructuring-assignment/correctness-v1`. Replay with
`node benchmarks/destructuring-assignment/correctness-v1/verify.mjs`.
It includes the initial 952-test all-target run and all nine final assignment
tests under strict Miri. Earlier MSRV results cover eight tests. These records
do not establish native speed or complete Test262 qualification.

Computed member targets retain the key value in their Reference; conversion
to a property key happens at PutValue, after source reads and defaults. The
reference-order regression also checks that a null base rejects the write
without converting its key, while closing an unfinished iterator.
