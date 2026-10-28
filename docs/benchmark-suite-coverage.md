# Benchmark coverage and remaining runtime gaps

This matrix describes executed paths and recorded limits for #427. It is a
coverage inventory, not a combined performance score. Input/source revisions,
binaries, compilers and machines remain attached to each original report;
results from different hosts or source stages cannot be pooled into a win.

| Group | Executed API and retained evidence | Scope still missing |
| --- | --- | --- |
| parse | Public `parse_script` and default `Engine::compile`, Unicode source, three rotated rounds on three hosts; [components](component-benchmarks.md) | No equivalent compile-only boundary in the actual pinned JsRuntime comparison; no cross-engine parser-speed claim |
| startup | Fresh `Engine::default` + Realm creation, every instance validated; [components](component-benchmarks.md). Actual-Boa workers separately record JsRuntime startup | Component timers exclude process spawn and probe/teardown. DOM/bootstrap and Realm startup differ and are not presented as equivalent |
| objects | Actual JS getter/write/delete cycles in components; original-eleven mono/mega/prototype/object allocation [actual-Boa records](typed-number-dispatch.md) | No application-wide object performance claim or inference about arbitrary shapes |
| async | Actual scheduling, job drain and completion, independent numeric/rejection oracles; [checkpoint suite](async-workloads.md), [later source stages](typed-number-dispatch.md) | Some later reports have three noise decisions; pending-await/full Promise compatibility and application superiority are unproved |
| RegExp | Actual JS construction/global exec/Unicode captures, [warm cache suite](regexp-compilation-cache.md) and [fresh unique patterns](regexp-ascii-compilation.md) | Conservative parser fallback remains; these fixed tests are not complete ECMAScript RegExp conformance |
| GC component | Real standalone tracing `Heap` with rooted/unrooted cycles, weak handles, minor barrier and actual destructor counts; [components](component-benchmarks.md) | These native graph lifecycles are not JavaScript Engine collection or GC pauses |
| Engine GC | Shared tracing heap, public `Engine::collect_garbage`, allocation safepoints and live-root/reclamation tests; all seven strict [ownership fixtures](runtime-ownership.md) release. The [original-eleven diagnostic profile](../benchmarks/runtime-gc/runtime-profile.json) separates allocations, root activity, peak live cells, collection pauses and remaining evaluation time | The diagnostic profile is one instrumented local ARM64 pass, not repeated cross-engine GC timing. The new source6c3 component cohort on ARM64/x64 passes all seven actual-Engine reclamation fixtures; the older main cohort describes the pre-migration source; generational/incremental runtime collection and arbitrary application GC performance remain unproved |
| CLI integration | Actual binary file/-e/stdin, shared Script globals, canonical module graphs, output/errors/jobs and pinned Test262; [shell](cli.md) | No dynamic import, package/event loop or new collector feature is implied |
| SpiderMonkey reference | Fixed native shell, disabled-tier and default-settings original-eleven [comparisons](spidermonkey-workloads.md), real adapter checks, including the traced-runtime follow-up | Both measured source stages lost all66 pair decisions. Authored phase/async/RegExp/GC shell comparisons remain incomplete; neither comparison establishes arbitrary application performance |

The exact-Intel arithmetic acceptance #510 is complete with separate native
actual-Boa comparisons and an original-input instruction diagnostic; see
[actual Intel evidence](../benchmarks/intel-arithmetic/README.md). Earlier noisy,
slower and unavailable-hardware reports remain preserved. Authored async can
have six faster/three noise decisions even when another earlier source/host
passed all nine; neither noise nor a failed/unreclaimed case is a speed win.

The historical component cohort has54 isolated workers and864 checked
warmup/sample outcomes across local ARM64, hosted ARM64 and hosted x64. Its
native cases are correct, but its pre-migration Engine probes fail reclamation;
those records are preserved. The migration branch separately passes strict
reclamation and records integrated Engine collection in the profile above.
Earlier competitive source stages qualified the uninstrumented original-eleven
comparison against actual Boa under the documented guest-CPU0 condition. The
latest source6c3 cohort records32 faster/1 noise decisions and does not qualify
all33 on that local machine. Its independent hosted follow-up on AMD9V45 and
AMD7763 qualifies all33 actual-Boa comparisons per host, while preserving main
regressions and same-binary variability. Unqualified/unaffined runs and
material regressions against main are retained alongside these observations.
These newer results do not retroactively change the historical component
cohort or establish cross-engine GC pause superiority. #427's [phase evidence map](../benchmarks/runtime-gc/experiments/primitive-prototypes-6c3/phase-acceptance-map.json)
records each stage separately. The current original-eleven Engine GC profile
also has a clean-source/compiler/machine/affinity record and independent
root/pause checks; it is a single instrumented pass, not cross-engine GC
performance proof. The latest119 exact library Miri executions and all19 security jobs pass;
final integration and actual merge remain required before completion.
