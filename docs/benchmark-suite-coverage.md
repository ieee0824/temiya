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
| Engine GC | Actual public-Engine handle/clone/teardown [retention probe](runtime-ownership.md), strict exit2 retained on all component hosts | No Engine force-collection hook; self-cycle/two-object-cycle/escaped-prototype survive teardown. Runtime migration #257/#313/#340 remains incomplete |
| CLI integration | Actual binary file/-e/stdin, shared Script globals, canonical module graphs, output/errors/jobs and pinned Test262; [shell](cli.md) | No dynamic import, package/event loop or new collector feature is implied |
| SpiderMonkey reference | Fixed native shell, disabled-tier and default-settings original-eleven [comparisons](spidermonkey-workloads.md), real adapter checks | Recorded source lost all66 pair decisions. Later production changes do not retroactively update those results; authored phase/async/RegExp/GC shell comparisons remain incomplete |

The exact-Intel arithmetic acceptance #510 is complete with separate native
actual-Boa comparisons and an original-input instruction diagnostic; see
[actual Intel evidence](../benchmarks/intel-arithmetic/README.md). Earlier noisy,
slower and unavailable-hardware reports remain preserved. Authored async can
have six faster/three noise decisions even when another earlier source/host
passed all nine; neither noise nor a failed/unreclaimed case is a speed win.

The component cohort has54 isolated workers and864 checked warmup/sample
outcomes across local ARM64, hosted ARM64 and hosted x64. Its native cases are
correct while the separate Engine reclamation gate remains false. This matrix
therefore leaves Engine GC measurement/integration explicit in #427's remaining
audit. It neither marks that gap complete nor substitutes a standalone Heap
benchmark for integrated JavaScript collection.
