# Bounded RegExp compilation reuse

RegExp construction reuses immutable compiled matcher strategies, while every
constructor still creates a new JavaScript object, independent `lastIndex`, a
fresh matcher search-cache pool and its own reusable capture buffer. Pattern and
flag conversion, scalar-string validation and flag parsing precede lookup;
invalid patterns are not cached. Cached templates are never searched, so input
strings and growing search scratch do not accumulate in the retained templates.
A successful execution returns owned captures before any replace callback runs.

The cache lives on the current thread, including across Engine/realm lifetimes.
It retains at most 256 entries, evicts the least recently used entry, and has an
8 MiB **approximate accounting budget**. Accounting includes the dependency's
compiled-strategy and fresh search-cache heap estimates, the source key and entry
size. These estimates exclude allocator overhead and some metadata; 8 MiB is
not a hard RSS limit. Live RegExp objects own their matcher after eviction and
are outside this cache-retention budget. Dropping a thread drops its cache.

The matcher uses the same regex-automata engine underlying regex::Regex, with
its existing 10 MiB NFA compilation limit, 2 MiB hybrid-cache setting and UTF-8
policies. Bounded backtracking and inline optimizations remain enabled; eager
DFA, hybrid, literal-prefilter and one-pass acceleration features are disabled.
No package version changed in Cargo.lock. This does not add missing ECMAScript
RegExp syntax, observable properties or full UTF-16 matcher semantics.

## Recorded ARM64 results

The frozen candidate is `bea78c9c824855dd3eb0e0aca7332808ab3c7da5`.
Its baseline runtime is `37e0996850829fdaa71bae34e99e8d04edd189b9`,
identical to main `3f9d1ae802691e5bf7157f2ee05a88e724736a97` for the
runtime, dependencies and worker. Both use rustc 1.99.1 release, no Temiya features,
and the public default Engine, verified as Bytecode for these inputs. Actual Boa
is the public JsRuntime in pinned omoikane
`007556adccb8fb4329236932ca8175f8e8d119d2`, with its real fork, lock,
features and sandbox. The frozen historical adapter's formatting-only source
identity is recorded in [typed Number evidence](typed-number-dispatch.md).

All fixed-suite reports use 15 samples plus a checked warmup, three alternating
pairs, unchanged inputs/oracles and the existing MAD-based threshold. Smaller
ratios mean less elapsed time. Linux ARM64 reports an unknown CPU model and four
logical CPUs; this is not a named-hardware claim.

| Input | Candidate / current main | Candidate / actual Boa |
| --- | --- | --- |
| RegExp construction, warm repeated patterns | 0.076 / 0.065 / 0.064, all faster | 0.568 / 0.573 / 0.526, all faster |
| Global exec | 1.035 / 1.064 / 1.063, all within noise | 0.754 / 0.725 / 0.725, all faster |
| Unicode captures | 1.036 / 0.998 / 1.038, all within noise | 0.808 / 0.815 / 0.813, all faster |
| Original eleven inputs | All 33 within noise | All 33 faster |

“Construction” includes string generation, `new RegExp`, exec and capture
lengths. Each pass has 128 distinct patterns, but subsequent passes repeat them;
the fixture explicitly permits caches. Its first cold pass remains recorded as
`warmup_ns`, outside the reported medians. This is not a compile-only benchmark.

A separate cold/unique diagnostic uses 2,048 constructions per pass, with fresh
pattern text for every construction across all 16 checked passes. All nine
workers return the independent result `2048*(8+5+2)=30720`. Candidate/current-main
ratios are 0.269 / 0.271 / 0.269, but candidate/actual-Boa ratios are
**1.812 / 1.937 / 1.809, all slower**. The candidate's sampled whole-process HWM
is 5,364–5,380 KiB after 32,768 unique patterns per worker, versus 4,576–4,608 KiB
for current main and 81,600–81,608 KiB for actual Boa. These sampled process peaks
include setup and warmup, and do not establish retained heap or allocation cost.
Short warm-suite workers can exit before a 25 ms memory poll: their missing
observations stay null, never zero. Cold construction remains a performance gap.

## Validation and remaining work

Public API tests cover new-object/state separation, flag distinctions,
conversion order on cache hits, original thrown values and capture-buffer reset.
Native stress tests exercise both entry eviction and byte-budget eviction:
32 large compiled patterns retain 21 entries with 8,280,804 accounted bytes.
The large-state byte-budget stress is excluded under Miri; the small-pattern
state/eviction test runs there. Eight pinned Test262 files execute in AST and
Auto, strict and sloppy, twice each: 64 passes, no failures or skips in this
selection. This is targeted regression coverage, not full RegExp conformance.

The initial source passed stable/MSRV 1.85, fmt/Clippy, C ABI, dependency audit,
AddressSanitizer and Miri. Newly added tests and evidence still need final CI.
Two independent hosted x64 comparisons, including same-binary and baseline/Boa
controls, are running. PR #515 remains a draft until those results and final
checks are reviewed. Intel issue #510 and broader issue #427 remain separate.

[Raw observations, rejected prototypes and offline verification](../benchmarks/regexp-cache/README.md)
retain all outcomes, including regressions and the remaining cold/unique gap.
