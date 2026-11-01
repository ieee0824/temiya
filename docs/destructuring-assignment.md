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

Verified bytecode lowers assignment patterns to `AssignmentPattern` metadata
consumed by `InitializePattern`. It preserves the RHS on the operand stack and
uses the same Reference writer as the interpreter. Assignment metadata permits
member targets without weakening declaration binding validation, cannot be
loaded as a runtime value, and participates in argument observation and
per-iteration environment analysis. `tests/bytecode_assignments.rs` checks
these contracts and that all six frozen benchmark functions compile to VM bodies.
Mapped sloppy arguments (#166), AST for-let closure capture (#173), and
optional Annex B var aliases (#174) remain separate work.

Public Engine tests exercise Ast and Auto in
`tests/destructuring_assignment.rs`. The implementation is pending fixed
Test262, safety, toolchain, and native Boa performance qualification. It does
not establish complete for-of or generator support. Issues #162 and #163 have landed; #164 still requires qualification
of its current runtime source.

Specification: [ECMA-262 destructuring assignment](https://tc39.es/ecma262/2026/multipage/ecmascript-language-expressions.html#sec-destructuring-assignment).

Local correctness evidence is retained in
`benchmarks/destructuring-assignment/correctness-v1`. Replay with
`node benchmarks/destructuring-assignment/correctness-v1/verify.mjs`.
It includes the initial 952-test all-target run and all ten final assignment
tests under strict Miri. Earlier MSRV results cover eight tests. These records
do not establish native speed or complete Test262 qualification.

Computed member targets retain the key value in their Reference; conversion
to a property key happens at PutValue, after source reads and defaults. The
reference-order regression also checks that a null base rejects the write
without converting its key, while closing an unfinished iterator.

The first historical instruction captures for all six unchanged inputs are
retained in `benchmarks/destructuring-assignment/vm-instructions-v1`. Replay
with `node benchmarks/destructuring-assignment/vm-instructions-v1/verify.mjs`.
The comparison includes parent-main integration and VM lowering, so it does
not isolate either change. All six recorded instruction totals decreased
(4.8–22.5%); this is not a native timing or Boa speed qualification.

Current-source local VM correctness records are retained separately in
`benchmarks/destructuring-assignment/vm-correctness-v1`; its verifier replays
the 1,013-test native result, four dedicated strict Miri tests, Clippy and the
original 46-variant loop audit, including its ten dependency failures. The
initial isolated-filesystem Miri rejection is retained alongside the successful
embedded-input run. Complete security and full-corpus checks remain pending.
