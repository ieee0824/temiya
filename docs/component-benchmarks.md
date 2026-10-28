# Checked parse, startup, object and collector measurements

The component suite supplements the unchanged original-eleven actual-omoikane
and SpiderMonkey comparisons, and the authored async/RegExp suites. It uses
public parser, Engine and Heap APIs with independent, checked outcomes.
It does not turn native component observations into a cross-engine victory.

| Case | Timed boundary | Independent checked outcome per pass |
| --- | --- | --- |
| parser-script | 1,024 parses, AST body checks and owned AST drops | 2 statements per parse, sum 2,048 |
| engine-compile | Sum of 128 `Engine::compile` intervals | Every compiled Unicode function uses Bytecode and evaluates to 42; sum 5,376 |
| engine-startup | Sum of 64 Engine construction + Realm creation intervals | Every fresh Realm evaluates `40+2` to 42; sum 2,688 |
| objects-properties | Invocation parse/evaluation and Script drop, setup excluded | 256 object/getter/write/delete cycles, sum `256*257 = 65,792` |
| gc-full-cycle | 16 complete standalone Heap graph lifecycles, including validation | Each rooted 256-node cycle survives tracing, then loses every node after root drop; 4,096 actual destructors |
| gc-minor-barrier | 64 standalone old-to-young graph lifecycles, including validation | Remembered young target survives minor collection, then both nodes are reclaimed; 128 actual destructors |

Compile evaluation/Script drop and startup correctness probes/teardown are
outside their named intervals. Their raw samples sum all intervals within one
pass; they are not process-start or whole-program elapsed times. Parser and
collector cases explicitly include their validation/drop costs. Object setup
runs once; each pass allocates its own objects and uses real JavaScript property
operations and getter calls. The original omoikane eleven remain unchanged.

Every isolated worker performs one checked warmup followed by 15 checked
samples. Three serial rounds rotate the six-case order deterministically.
Median, median absolute deviation and relative MAD are retained for each round,
without filtering samples or selecting a favorable round. A high MAD remains
visible; there is no comparison ratio, source-speedup label or cross-engine pass
for these single-engine component records. Machine/revision/compiler/build
flags, exact input source, worker/source hashes, raw output and exit status are
recorded. The 180-second / 512-MiB Linux `/proc` watchdog polls every25ms; it can
overshoot between polls. An unobserved peak remains null, never zero. Process
HWM includes setup and allocator state and does not measure retained JS heap.

## GC scope and actual runtime retention

`temiya::gc::Heap` really traces explicit roots, reclaims cycles and checks a
remembered set, weak handles and graph integrity in these measurements. It is a
standalone API, with no equivalent Engine force-collection hook. Its timings
cannot represent JavaScript GC latency or demonstrate application reclamation.

Each report also runs the existing public-Engine `ownership_probe` in strict
mode before measurement, preserving its actual exit code, raw observations and
`reclamation_ready` result. A strict failure remains a failure of Engine
reclamation readiness even when every standalone Heap measurement is correct.
The driver accepts the probe's documented code2 as an observation, not a
reclamation pass. Current self-cycle/two-object-cycle/escaped-prototype retention
must remain visible; this suite does not implement the ownership migration.

## Reproduction and checks

Start from a clean commit, with a release Rust compiler and Linux. Build workers
before starting any measurements, into a new output directory:

```sh
node scripts/build-component-worker.mjs /tmp/temiya-components-frozen
node scripts/check-component-workers.mjs \
  /tmp/temiya-components-frozen/target/release/examples/measure_components
node scripts/measure-components.mjs /tmp/temiya-components-frozen/build-info.json \
  > /tmp/components.json 2> /tmp/components.progress
node scripts/verify-components.mjs /tmp/components.json
```

The worker validates actual parse shapes, Engine results/backends and actual
collector retention/reclamation/drop results. The checker runs all six cases and
rejects six invalid commands and42 mutated result contracts. CI executes these
actual-worker checks on stable and MSRV1.85; the optional component workflow
records independent x64 and ARM64 hosts, with artifacts retained on failures.
A worker failure, wrong result, invalid provenance, timeout or memory failure
stays a failure. The offline verifier recomputes complete coverage, rotation,
checked outcomes, raw-output consistency, medians/MAD and reclamation readiness.

Existing parser, embedding, property and collector regressions cover the APIs
used here; no new engine syntax is introduced. CLI pinned Test262 integration
and existing fixed numeric/RegExp/object suites continue to run in normal CI.
The new component worker introduces no changes to Engine execution, compiler
optimization, fixture bodies, numerical oracles or comparison thresholds.

This advances #427's dedicated phase groups. The standalone GC qualification,
actual Engine retention limitations and broader application/SpiderMonkey goals
must be considered explicitly in its completion audit, rather than inferred
from a successful benchmark workflow.

## Recorded first cohort

Source6ec1c50 was measured locally on ARM64 and in hosted run40620663377 on
Intel Xeon Platinum8573C x64 and another ARM64 host. All54 isolated workers
completed864 checked pass-level results. The Engine ownership probe returned
strict code2 on all three hosts, retaining self-cycle, two-object-cycle and
escaped-prototype; Engine reclamation readiness remains false.

[Raw data, all round medians/MAD, provenance and independent verification](../benchmarks/components/README.md)
retain these failures alongside successful standalone Heap observations. The
library/runtime/test/Cargo source paths are unchanged from main2f0f4c2; this is
a suite/evidence change with no engine optimization or cross-engine speed claim.
