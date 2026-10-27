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

Implementation is pending actual worker compilation, integration checks and
measurements; Node syntax checks do not qualify this suite. No async speedup,
full Promise/await conformance, garbage collection completion or application-wide
advantage is established. Dedicated parse/startup/objects/RegExp/GC groups and
the remaining SpiderMonkey checks remain separate work under #427.
