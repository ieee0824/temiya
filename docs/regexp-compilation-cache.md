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

## Hosted x64 confirmation

Run 40584186976 records two independent AMD EPYC 7763 machines. These are two
machines of one CPU model. Workflow-only source a9bb0a7 has the same runtime,
worker and dependencies as bea78c9; the baseline is exactly main 3f9d1ae.
Each cohort builds the actual pinned Boa embedding with the same compiler and
records both baseline/Boa and identical-executable controls.

Construction candidate/Boa ratios are 0.534 / 0.544 / 0.520 on copy 1 and
0.518 / 0.506 / 0.534 on copy 2: all six faster beyond noise. All other twelve
RegExp/Boa comparisons and all 66 original-eleven/Boa comparisons are faster.
Construction candidate/main ratios are 0.059 / 0.060 / 0.059 and
0.059 / 0.057 / 0.069. The other twelve RegExp before/after comparisons have
11 noise outcomes and one faster outcome, with no detected regression.
Original-eleven before/after outcomes are 64 noise and two faster, with no slower
outcome; these do not establish a general source speedup outside RegExp.

Identical-executable RegExp controls are all 18 within noise. Original-eleven
controls have 65 noise and one slower outcome: copy 1 array pair 1 has ratio
1.420, despite identical source and executable hashes. This is a recorded
variability observation, not a code regression or an omitted result. Baseline/Boa
controls reproduce the original RegExp construction gap on both machines and
pass all original-eleven comparisons. All complete reports and worker identities
remain in the [hosted evidence](../benchmarks/regexp-cache/hosted/).

Across ARM64 and these x64 cohorts, the candidate has 27 authored RegExp and
99 original-eleven comparisons faster than actual Boa. Cold/unique construction
remains slower, as reported above. Neither these machines nor these inputs
resolve the exact Intel Xeon 6973P-C arithmetic condition in #510.

## Validation and remaining work

Public API tests cover new-object/state separation, flag distinctions,
conversion order on cache hits, original thrown values, capture-buffer reset,
UTF-16 boundaries and the existing lone-surrogate error/flag-validation order.
Native stress exercises entry and byte-budget eviction: 32 large patterns retain
21 entries with 8,280,804 accounted bytes. Large-state byte stress is excluded
under Miri; the smaller-pattern state/eviction test runs there. Eight pinned
Test262 files execute AST/Auto, strict/sloppy and twice each: 64 passes without
failures or skips in this selection. This is targeted coverage, not full RegExp
conformance.

Recording source 2b2a690 passed all four CI jobs and all three security jobs,
including both toolchains' actual 64-pass Test262 step. See [PR #515 checks](https://github.com/ieee0824/temiya/pull/515/checks) for
validation of the final evidence/UTF-16 test recording commit. #516,
#510 and broader #427 remain open. The follow-up ASCII-HIR prototypes for #516
are separate experiments and are not part of this change.

[Raw observations, rejected prototypes and offline verification](../benchmarks/regexp-cache/README.md)
retain every outcome, including regressions, control variability and the cold gap.

## Cold/unique follow-up diagnosis (#516)

Run 40586406012 profiles the exact 2,048-construction cold/unique input under
Callgrind on AMD EPYC 7763 x64. Workflow-only source 7130f33 has the same runtime,
worker and dependencies as the recorded candidate. Warmup and sample each return
30720 through Bytecode; together they construct 4,096 fresh patterns.

Whole-process instruction references are 593,647,915. Inclusive attributions
include matcher build 349,983,727 (58.95%), AST parsing 108,999,858 (18.36%), HIR
translation 58,951,246 (9.93%), NFA construction 91,633,554 (15.44%), and cache
lookup 50,966,838 (8.59%). These overlap: they must not be added as disjoint costs.
They include startup/setup and both checked passes, and establish neither native
cycles, exclusive costs, ARM elapsed-time shares nor a passing speed gate.
[Raw profile and verification](../benchmarks/regexp-cache/cold-unique/profile/)
identify fresh-pattern compilation and lookup as concrete follow-up work in #516.
