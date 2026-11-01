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

`vm-gc-correctness-v1` adds one explicit Bytecode-backend test of ephemeral
assignment targets across reentrant collection from setters, getters, defaults
and rest writes. Native execution and strict Miri both pass, and the final
collection retains zero allocations. Its verifier proves that compiler inputs
match the measured runtime source; it supplements the four VM tests above.

All ten AST/Auto assignment regressions also pass strict Miri on the current
VM runtime; their exact test names and complete log are replayed by
`vm-correctness-v1/verify.mjs` alongside the four dedicated VM tests.

The complete current-VM Test262 run (41303173669, source eb29c416) covers all
52,133 original files and 100,211 variants: 38,759 pass, 52,865 fail, 18 skip
and 8,569 timeout, with no missing or unreported variants. It preserves all
38,758 prior assignment-source passes and all 37,919 qualified main/object
passes, with zero regressions and one additional assignment-source pass.
`full-initial-v1` and `full-vm-v1` retain the complete prior/current captures;
`full-vm-v1/verify.mjs` independently replays all eight shards for both sources
and qualified main, then reconstructs the per-variant comparisons. The
remaining failures and timeouts are retained and do not establish complete
ECMAScript conformance or native speed.

Current-runtime security run 41303175895 passes all 19 jobs. Its sixteen
strict Miri shards execute 173 unique exact library tests; ASan executes
1,012 tests, with the dependency audit and Miri aggregate both successful.
`security-vm-v1` retains every job log and the exact test list. Replay with
`node benchmarks/object-binding/security-prepared-guards-v1/verify.mjs
benchmarks/destructuring-assignment/security-vm-v1
eb29c4169c0fdfd9246546f76b5874097a53ef62` on a single command line. These
results qualify the recorded runtime source for the stated checks only;
native Boa speed remains pending.

The frozen142 text-matched assignment files contain219 variants:217 pass
and2 fail. Both failures are the strict/sloppy variants of
`for-of/body-dstr-assign-error.js`, whose custom iterator is not consumed by
the legacy for-of path; Iterator protocol and closing integration remain #171.
They remain failures in the complete capture. The inherited8,813-file selection
contains17,424 variants:10,245 pass,2,748 fail and4,431 timeout.
`selection-summary.json` preserves this full breakdown. The142 text matches
include one untagged duplicate-__proto__ case; actual assignment metadata tags
cover141 files and217 variants. No outcome-based selection is applied.

Both complete earlier native captures are retained by `verify-hosted.mjs`.
`hosted-initial-v1` preserves all80 reports from run41245424047;
`hosted-vm-v1` preserves all80 from run41303412892. The latter qualifies
most groups but fails the required two-of-three gate for object-binding abrupt
coercion on copy1 (one win) and assignment computed-default on copy2 (one win).
All inputs,15 samples,3 alternating pairs, thresholds and both original
unfiltered machines remain recorded. Integrity replay is not a speed pass.

The next source integrates direct verified VM strict comparisons and guarded
standard instanceof dispatch, including builtin defining-Realm error handling.
Its native, corpus, safety and speed results require fresh validation; results
above remain specific to their recorded sources. Assignment evidence has its
own CI shard, preserving every existing check and the20-minute job timeout.
