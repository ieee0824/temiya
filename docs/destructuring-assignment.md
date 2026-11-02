# Destructuring assignment

Array and object assignment patterns write through References and PutValue,
rather than declaration initialization. The assignment expression returns its
original right-hand value. Ast and Auto expose this behavior through Engine;
eligible Scripts and ordinary function bodies also execute through verified
bytecode. This does not establish complete for-of or generator support.

Target references precede the corresponding iterator step or source property
get. Defaults run after that read and before the write. Earlier writes remain
observable if a later target throws. IteratorClose preserves the original throw
completion. Object rest resolves its target before copying properties. Computed
member target keys remain in their Reference until PutValue, including across
source getters, defaults and reentrant collection.

Verified bytecode retains `AssignmentPattern` metadata consumed by
`InitializePattern`, while preserving the original RHS on the operand stack.
Assignment metadata allows member targets without weakening declaration binding
validation and cannot be loaded as a runtime value. It participates in argument
observation and per-iteration capture analysis. Complex patterns use the shared
runtime evaluator and Reference writer.

Static object assignment plans cache immutable names and target syntax only.
They retain no Realm, runtime Value or Reference. Each object level checks for
null or undefined before evaluating its targets. A nested object's parent read
precedes all child target evaluation. A leaf target retains its actual Reference
before its source read, so a getter that replaces a target binding still writes
to the previously resolved object. Primitive source reads preserve their receiver.
Computed keys, defaults, rest and unsupported static target shapes retain the
shared path.

A separate loop proof permits header-cell reuse around hoisted leaf functions
only when verified names cannot observe cloned header names or eval. Nested
closures, metadata bridges, calls and unlisted instructions remain conservative.
Body environments, functions and prototypes retain distinct identities. Public
prepared-frame execution does not opt into this reuse.

`tests/destructuring_assignment.rs`, `tests/bytecode_assignments.rs` and
`tests/bytecode_assignment_gc.rs` cover public execution, metadata validation,
evaluation order, partial writes and reentrant GC. The current nested-plan source
is `eb7f054d7db7256c3d228849a424758fac45f04e`. Its local all-target/all-feature
run passes 1,024 tests with 26 ignored; fmt and Clippy pass. Three focused strict
Miri tests cover prepared plan use, nested evaluation order and collection across
parent reads and child references. Complete logs and all 83 compiler-input hashes
are retained in `benchmarks/destructuring-assignment/nested-assignment-correctness-v1`.
Run its `verify.mjs` to replay the evidence.

Full Test262 run 41362294489 covers all 52,133 files and 100,211 variants:
38,759 pass, 52,865 fail, 18 skip and 8,569 timeout. No variants are missing or
unreported. It preserves all 38,759 passes of source a151aedc, with no regressions
or additional passes. `full-nested-assignment-v1/verify.mjs` independently checks
every shard, replays the known-pass gates, compares every variant with the preceding
source and reconstructs the feature selection. These results do not establish
complete ECMAScript conformance.

The unchanged 8,813-file selection contains 17,424 variants: 10,245 pass,
2,748 fail and 4,431 timeout. Its 142 text-matched assignment files contain
219 variants: 217 pass and 2 fail. Both failures are the Sloppy/Strict variants
of `for-of/body-dstr-assign-error.js`, tracked by #171 for Iterator protocol
and closing integration. They remain failures. Metadata tags alone cover 141
files and 217 variants; the additional text match is an untagged duplicate
`__proto__` test. No outcome-based selection is applied.

Current-source security run 41362296996 passes all 19 jobs: 177 unique exact
library Miri tests, 1,023 ASan tests, dependency audit and the Miri aggregate.
The exact Miri names match all 177 enumerated library tests. Complete logs and
the source-specific verifier are retained in `security-nested-assignment-v1`.
Two-runner native measurement 41362299598 is still pending. Local correctness,
passing CI, full-corpus and security results do not replace the native speed gate. CI run 41362630287 passed all 11 jobs, including stable,
MSRV, C ABI and all seven evidence suites; all 83 compiler inputs at its data-only
head match eb7f054d. Issue #164 remains open until source-specific native speed qualification is
complete.

All six unchanged instruction captures are retained in
`nested-assignment-instructions-v1`. The nested object case uses approximately
9.6% fewer instructions than source 79df851; the other five did not improve.
The preceding source's six adverse captures remain in
`static-assignment-instructions-v1`. Instruction counts are not native timings
or a Boa speed qualification.

Earlier native captures remain in `hosted-initial-v1` (run 41245424047),
`hosted-vm-v1` (41303412892) and `hosted-comparisons-v1` (41307560987).
Each retains all 80 reports from both original unfiltered jobs, with unchanged
inputs, 15 samples, three alternating pairs, controls and thresholds. Each fails
at least one required two-of-three speed gate. `verify-hosted.mjs` accepts these
adverse observations for integrity replay and does not turn them into speed passes.
Source a151aedc's full-corpus and security records are retained separately in
`full-comparisons-v1` and `security-comparisons-v1`: all 100,211 variants,
175 exact library Miri tests, 1,017 ASan tests and all 19 security jobs.
These qualify only that recorded source for their stated checks.

Earlier local and corpus records remain in `correctness-v1`, `vm-correctness-v1`,
`vm-gc-correctness-v1`, `comparisons-correctness-v1`, `full-initial-v1` and
`full-vm-v1`. Their verifiers preserve original failures, skips and timeouts;
their results are historical rather than transferred to a changed runtime.

Mapped sloppy arguments (#166), complete for-of integration (#171), AST for-let
closure capture (#173) and optional Annex B var aliases (#174) remain separate
work. Issues #162 and #163 have landed. Specification:
[ECMA-262 destructuring assignment](https://tc39.es/ecma262/2026/multipage/ecmascript-language-expressions.html#sec-destructuring-assignment).
