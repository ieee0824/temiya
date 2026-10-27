# Async checkpoint workloads

This authored suite adds three async workloads alongside the unchanged original
eleven omoikane inputs. Each invocation schedules 2,048 operations. The chain
increments from zero, fanout sums integers 0 through 2,047 and verifies all
2,048 completions, and rejection recovery carries each increment as the actual
numeric throw value. The independent oracles are 2,048, 2,096,128 and 2,048.
The definitions and source hashes are frozen in the extracted manifest.

The workers use `Engine::default` / `Engine::drain_jobs` and the pinned
application's actual `JsRuntime::eval` / `JsRuntime::run_jobs`. The native Boa
context is not substituted for the application embedding. Its vendored sources,
original lock, features and sandbox remain those of the original comparison.
The default synchronous adapter is unchanged; an explicit `async` build kind
selects the new adapter source and records its hash.

Every checked pass parses and evaluates a fresh scheduling invocation, verifies
the pre-checkpoint sentinel, drains the real host job queue, and parses and
evaluates the completion lookup. The interval includes all these operations.
Setup, engine startup and the checked warmup are recorded separately and
excluded from samples. Temiya reports actual drained job counts; the public
application returns unit, so its queue effects are validated by the independent
JavaScript completion and reaction-count oracle. No fabricated job count is
reported. Promise allocation and any ownership retention are part of execution;
these are not GC-completeness benchmarks.

Both release adapters are exercised before timing, including correct queued
results, signed zero, a wrong pre-checkpoint sentinel, wrong completion,
failure after a correct warmup, a throwing callback and a throwing completion
lookup. A failing adapter emits no timing report.

The opt-in `async-benchmark.yml` workflow builds both actual workers before
serial measurement, uses the same Rust compiler, runs 15 checked samples and
one checked warmup with three alternating pairs, and saves raw samples,
median/MAD, binary/source/compiler identity and machine details. A 512 MiB RSS
watchdog and 180-second process deadline apply to both workers; the application's
additional native sandbox remains recorded. RSS can overshoot between polls.
Failures, unsupported behavior, timeouts and missing callbacks cannot be wins.
The original 10% / three-MAD noise rule remains in force.

The actual release workers and all 14 native adapter checks passed in
[run 40538696453](https://github.com/ieee0824/temiya/actions/runs/40538696453),
measured at 010fcc45cb9231b44e0acec5fbf10a5290465a25 on Linux x64,
AMD EPYC 7763 (four logical CPUs), rustc 1.99.1. All 18 workers and 288 results
were correct; all nine comparisons were faster for Temiya beyond the fixed
noise rule. Input hashes, independent oracles, unique full coverage, alternating
order, compiler/adapter/binary identity, median/MAD and every comparison decision
were independently verified. Temiya setup actually selected AST; the report
also records invocation/completion backends and actual drained job counts.

| Authored case | Temiya / actual Boa median elapsed ratio, three pairs |
|---|---:|
| Promise chain | 0.5595–0.6163 |
| Promise fanout | 0.8043–0.8346 |
| Numeric rejection recovery | 0.7108–0.7306 |

Peak sampled Temiya HWM was 44,448–53,592 KiB for the chain, 95,172–101,140 KiB
for fanout and 105,484–105,648 KiB for rejection recovery. Actual Boa ranged
84,576–85,452 KiB across these cases. Temiya therefore used more observed process
memory in two cases; the speed result is not a memory or collector advantage.
Promise ownership and cyclic retention remain unfinished work, and no forced
collection or missing job was substituted for actual execution.

[Raw measurements](../benchmarks/async-checkpoint/async.json),
[actual positive/negative adapter checks](../benchmarks/async-checkpoint/worker-checks.json)
and [provenance](../benchmarks/async-checkpoint/provenance.json) retain the result.
These three inputs do not cover pending-await suspension or the full Promise API,
and establish no application-wide advantage. The separate Intel arithmetic
shortfall (#510), dedicated parse/startup/objects/RegExp/GC groups and remaining
SpiderMonkey checks remain work under #427.
