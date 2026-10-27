# Fresh ASCII RegExp compilation

This experiment addresses #516, where every pattern text is fresh across all
checked passes. It is separate from the warm compilation cache in #513.

A conservative ASCII grammar builds regex-syntax HIR directly. It supports
literal runs, positive ASCII ranges, numbered captures, noncapturing groups,
alternation and greedy/lazy repetition. Unrecognized forms, non-ASCII patterns,
case-insensitive patterns and deeper nesting use the existing full parser.
Partial parsing has no JavaScript effects. Flags, scalar validity, coercion order,
new object identity and independent lastIndex remain required.

The direct program uses the same Thompson NFA semantics as the existing matcher.
Its bounded backtracker retains the 256 KiB visited budget; capacity exhaustion
lazily creates the linear PikeVM over that same immutable NFA. Each JavaScript
object owns its search storage. Compiled cache templates never search input.

The cache uses owned UTF-16 keys, full-key checks after randomized hashing,
linked LRU slots, 256 entries and the existing 8 MiB approximate accounting
budget. Short ASCII source (at most 4096 code units) can reuse a compiler with a
128 KiB intermediate-state limit. A failed small compile discards its scratch
and retries a fresh compiler with the existing 10 MiB limit. Longer source goes
directly to the fresh compiler. The source bound also bounds capture metadata;
these budgets are not hard process RSS limits. Immutable execution builders
reuse input-independent initialization only. Objects and completed programs
survive cache eviction and compiler resets independently.

Numeric conversion uses the same ryu-js formatting and special-value policies,
encoding its borrowed output directly into JsString. Builtin exec builds its
capture values directly, while the public RegExpMatch API retains owned values,
UTF-16 index and named captures. Internal conversion callbacks never execute
JavaScript; result arrays remain independent across searches.

Native coverage includes 1764 generated grammar combinations under both Unicode
policies, 515088 comparisons of captures and scalar start offsets against the
full parser, hash collisions and both eviction paths, scratch-limit fallback,
old-program lifetime, long-search fallback, UTF-16 boundaries and existing
surrogate errors. Generated/large-state stress runs natively; small fixed
regressions also run under Miri when that check is executed. Eight fixed Test262
RegExp files produce 64 selected executions; the typed-Number selection adds 20.
These are targeted checks, not whole-engine or full RegExp conformance.

## Measurements in progress

Exact input: 2048 fresh constructions per call, 15 samples plus checked first
pass, three serial rotations of candidate/current main/actual modified Boa.
Each worker constructs 32768 distinct patterns and checks result 30720 on all
16 passes. Input/source/oracle/compiler/binary/fork provenance is checked by
scripts/measure-regexp-cold-diagnostic.mjs; missing results never pass a gate.

Initial runtime 7e75280 (run 40597703258) passed all three actual-Boa
comparisons on each of two AMD x64 runners, including repeat observations.
These results do not qualify later runtime revisions.

Runtime a8a824d, recorded by draft PR head 96a2960, has not passed the
cold-performance gate. Run 40598960611 recorded actual-Boa ratios
0.946633 / 0.882123 / 0.895645 on Intel 8370C and
0.908812 / 0.892956 / 0.904733 on AMD 7763. Each cohort contains
within-noise decisions under the unchanged minimum 10% threshold.
Dedicated ARM64 run 40599864531 recorded 0.944921 / 0.948538 / 0.944780
and 0.952971 / 0.954643 / 0.957900: all within noise, despite roughly
halving elapsed time relative to current main. Identical-executable controls
are within noise on both ARM64 runners. Raw reports retain checked first
passes, all 15 samples, independent oracle results and negative decisions.

Local ARM64 original-eleven before/after measurements also include one
object-alloc slowdown (ratio 1.133801); its cause remains unresolved. The
actual-Boa comparison has 31 faster and two within-noise decisions. Local
warm RegExp measurements have five faster and four within-noise decisions.
The completed local identical-binary control has 29 within-noise, two faster
and two slower decisions; object-alloc is within noise in all three pairs.
This demonstrates process variability but does not clear the retained source
regression. Hosted original-eleven regressions and controls are still running.

The subsequent short-ASCII subject experiment (581265a) avoids an intermediate
UTF-8 allocation for at most 64 ASCII code units, preserving the existing
scalar conversion for other subjects. Its local cold report is correct but
has no qualified actual-Boa wins. Boundary/capture ownership tests cover
63/64/65 code units and non-ASCII UTF-16 indexing. It is not yet adopted.

Before adoption: final-source cold measurements, warm authored RegExp and the
original eleven on ARM64 and x64, both compiler versions, C ABI, fmt/Clippy and
security checks must be reviewed. #510 and the broader performance/compatibility
scope of #427 remain separate.

The inline-scratch variant (40a4f58) keeps the bounded cache directly inside
the per-object search state, removing a separate Box allocation. Internal
RegExp tests and all-target/all-feature Clippy pass; its local cold report
passes correctness but misses the overall Boa gate (one faster, two noise).

The final instruction profile also identifies ordinary-property lookup cost.
Variant831dc98 adds a single position hint per object; every read rechecks
bounds and the full current key, then clones the current descriptor. Values
are never cached. Proxy/exotic operations and lazy prototype materialization
retain their earlier ordering. Deletion/redefinition/accessor/Symbol coverage
and the full native suite pass. Local cold results are correct but within
noise versus Boa; hosted final-source measurements remain necessary.
