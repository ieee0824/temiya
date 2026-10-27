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

Run 40597703258 copy 1 (AMD EPYC 9V45, source 786ae28, runtime equal to 7e75280)
records candidate/actual-Boa ratios 0.873979 / 0.898180 / 0.892383, all faster
beyond the existing threshold. The identical-executable control is within noise
in all three cases; its additional actual-Boa comparisons also pass. Copy 2 is
still running. The final boxing/metadata-guard recording changes require their
own measurements. Local ARM64 diagnostics have substantial noise and do not yet
establish all three actual-Boa wins; all such observations are retained.

Before adoption: final-source cold measurements, warm authored RegExp and the
original eleven on ARM64 and x64, both compiler versions, C ABI, fmt/Clippy and
security checks must be reviewed. #510 and the broader performance/compatibility
scope of #427 remain separate.
