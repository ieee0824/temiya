# Conformance and performance baselines

Run `scripts/bench`; it records the Git revision, `rustc` version, target
architecture, stable input identifiers, and all timing samples. The suite covers
Script parsing, parse-plus-interpreter macro execution, full GC, and Object
built-ins. `TEMIYA_BENCH_SAMPLES` and `TEMIYA_BENCH_ITERATIONS` are recorded
controls; reference runs use at least 15 samples and an otherwise idle runner.

Only compare runs from the same hardware, OS image, toolchain, profile, inputs,
and iteration count. Summaries use median and median absolute deviation (MAD).
A slowdown is a regression only when it exceeds both 10% and three times the
sum of baseline/current MAD-to-median ratios. Suspected regressions receive a
second run before filing; performance targets themselves are outside this policy.

`benchmarks/baseline.json` stores the reviewed reference environment, policy,
revision, samples, and results. Test262 conformance uses the separately pinned
revision and skip manifest in `test262/manifest.json`; changes to pass/skip/fail
counts must be reviewed alongside that manifest rather than mixed with timings.

## Cross-engine targets

The performance target is to exceed the Boa version embedded in
[omoikane](https://github.com/ieee0824/omoikane); exceeding SpiderMonkey is an
additional target. The default Engine's [Auto Script comparison](auto-backend-integration.md)
exceeds the actual pinned omoikane Boa on the original eleven workloads in all
33 comparisons on the original measured AMD x64 and ARM64 machines. This is
limited to those inputs and conditions, not complete application superiority.
A [subsequent Intel follow-up](promise-rejection-performance.md) did not pass
the all-eleven gate: arithmetic was slower than actual Boa in three pairs.
That target remains open in #510, alongside the separately preserved ARM result.
The [pinned native SpiderMonkey comparison](spidermonkey-workloads.md) completes
the same inputs in both modes, but Temiya is slower in all 33 comparisons per
mode. The SpiderMonkey speed target remains unmet; default-mode enabled JIT
settings do not establish workload JIT execution.

The additional [async checkpoint suite](async-workloads.md) measures real
scheduling, public job draining and checked completion in three authored cases.
The recorded AMD x64 run passed all nine actual-Boa comparisons, with higher
Temiya process memory in two cases. It does not change the original eleven
inputs or establish pending-await, collector or application-wide completeness.

The initial application reference is omoikane revision
`007556adccb8fb4329236932ca8175f8e8d119d2`, including its vendored `engine/boa`,
vendored `engine/regress`, embedding configuration, and feature flags. Its
`tests/js_benchmark/shapes.js` fixture version 2 has eleven workloads and
independent result checks in `tests/engine_benchmark_results.rs`. An upstream
Boa release is not a substitute for that modified engine. Record a pinned
SpiderMonkey revision/build and actual execution mode when adding its results.

For a reviewable comparison:

- Run the same workload sources and iteration counts on the same machine in
  release builds. Record source hashes, engine revisions, compiler/build flags,
  architecture, OS, and whether JIT execution actually occurred.
- Check the expected result on every warmup and measured pass. Unsupported
  inputs, incorrect results, crashes, timeouts, and skipped tests cannot count
  as speedups. Isolate workloads in processes so one failure does not erase
  the remaining diagnostics.
- Report startup/parse costs separately from warmed execution. Identify whether
  a run uses the actual application embedding or a standalone engine context;
  they answer different questions. Preserve each workload's samples, median,
  and MAD; do not hide regressions behind an aggregate score.
- Use repeated runs to establish improvement beyond measurement noise. Keep
  correctness/conformance results and memory growth alongside timing results;
  leaking allocations or dropping observable work is not an optimization.

The eleven microbenchmarks provide an initial comparison, not proof that a
complete application or every JavaScript program runs faster. Broader workloads
and the outstanding compatibility work remain necessary before such claims.

### Array workload diagnostic

The array stack implementation enables omoikane's original array function body
at its original 500,000 iterations. Extract it from the pinned Git objects of an
omoikane checkout, retaining its MIT notice, then run the numeric result worker:

```sh
node scripts/extract-omoikane-array.mjs /path/to/omoikane \
    > /tmp/omoikane-array.js 2> /tmp/omoikane-array-provenance.json
cargo build --locked --release --example measure_number
timeout 180 target/release/examples/measure_number \
    /tmp/omoikane-array.js 124999750000 3
```

Node.js only extracts the function; it does not run the measured loop. The
worker separately records Realm creation, compilation, one discarded warmup,
and evaluation samples through Engine. It validates every result, including
warmup, against the independent sum `n * (n - 1) / 2`. An incorrect result or
engine error exits unsuccessfully without a valid timing report.

This replaces omoikane's JavaScript timing harness with a Rust host timer. It
does not run the full fixture, DOM embedding, or Boa/SpiderMonkey comparison.
Three samples are a diagnostic baseline, not a performance acceptance gate;
the complete comparison still requires the controls described above. The
baseline predates the packed/holey/sparse array element implementation.

[The recorded diagnostic baseline](../benchmarks/omoikane-array-baseline.json)
retains the implementation revision, input hashes, compiler/machine information,
and all three samples. Its median is approximately 7.46 seconds on the recorded
ARM64 environment; this is an optimization starting point, not a speed target
achievement.

### Comparing array storage changes

Build `measure_number` for the baseline and current engine in separate Git
worktrees and separate Cargo target directories. Shared target directories can
reuse a fresh package artifact from the wrong checkout, invalidating results.
Then compare the workers serially, with alternating order between pairs:

```sh
BASELINE_REVISION=<full-baseline-commit> CURRENT_REVISION=<full-current-commit> \
    node scripts/compare-number-workers.mjs \
    /path/to/baseline/measure_number /path/to/current/measure_number \
    /tmp/omoikane-array.js 124999750000 15 3 > /tmp/array-comparison.json
```

Each worker validates one warmup and every timed result. The driver records
all samples, executable/source hashes, exact revisions, compiler, hardware,
per-pair median/MAD comparisons, and sampled Linux process memory. It exits
unsuccessfully without a comparison report if a worker fails or times out.
The workers must have been built with the reported compiler and no features;
the driver cannot recover build flags from an executable. Sampled process
memory is diagnostic and does not establish that all reference cycles are
collected. These are Temiya before/after measurements, not measurements of the
vendored Boa embedding or SpiderMonkey.

[The array storage comparison](../benchmarks/array-elements-comparison.json)
records three alternating pairs of 15 timed passes per revision, each preceded
by a validated warmup. On the recorded ARM64 machine, the freshly built
baseline medians were 3.31–3.37 seconds and packed/holey/sparse storage medians
were 0.985–1.035 seconds: a 68.8–70.2% reduction (3.21–3.36× speedup) on this
specific workload. Each pair exceeded the documented noise threshold. These
paired baseline runs, rather than the earlier 7.46-second standalone record,
are the evidence for improvement. All 96 executions produced the expected
124999750000 result. Sampled process high-water marks were 3,788–3,812 KiB
before and 3,544 KiB after; these observations are not a general memory bound.

The [internal regression diagnostic](../benchmarks/array-elements-internal.json)
also retains three alternating pairs for parsing, parse-plus-interpreter,
standalone GC, and ordinary object properties (15 samples, 1,000 iterations).
None crossed the regression policy threshold. Some medians increased, including
the interpreter macro by 3.5–13.2%; noise thresholds were 10.2–26.0% for those
pairs. This is limited evidence against a large regression, not an equivalence
claim or a whole-engine speedup.

### Omoikane application embedding comparison

The application worker in `benchmarks/omoikane-worker/main.rs` calls
`omoikane::js::JsRuntime::with_document_and_sandbox` and `JsRuntime::eval` with
a real document and the configuration from the pinned application's result
tests: 60-second cooperative deadline and 100,000,000 loop iterations. Its
vendored Boa/Regress paths, lockfile versions/checksums, and default application
features are preserved. Boa has annex-b, intl_bundled, experimental (Temporal),
float16, and xsum enabled; baseline-jit is absent. This is the actual embedding,
not an upstream Boa crate or standalone Context.

Extraction retains all eleven exact function bodies, original iteration counts,
and independent numeric oracles. The original shared makeShapes/ProtoBase/
protoReceiver text is instantiated once per worker in a stable closure rather
than in global slots. A setup assertion verifies prototype identity and method
behavior. This preserves one shared prototype receiver across all timed passes;
it is never recreated inside the body. Both engines receive the same setup
and invocation bytes. The original JavaScript timing/formatting harness is
replaced by Rust timers; no missing engine API is stubbed.

Each worker records runtime startup, setup (parse/instantiate), and one warmup
separately. Every timed pass parses and evaluates the tiny invocation, calling
the already-created function. The public application API does not expose a
parse-only or parse-free execution boundary, so these samples must not be
labeled pure execution time. Temiya deliberately recompiles the invocation on
every pass to match that boundary. The application startup includes DOM/web API
bootstrap, whereas Temiya startup creates its Realm; those startup numbers
describe different embeddings and are not an engine-only speed comparison.

The driver alternates engine order across three pairs, runs workers serially,
and validates every warmup and sample. A failed/incorrect result, crash,
180-second process timeout, or sampled RSS above 512 MiB is recorded separately
and never contributes a speed ratio. These watchdog settings apply equally to
both processes; on Linux the memory watcher polls every 25 ms and may overshoot.
Temiya currently lacks Boa's additional cooperative sandbox, which remains
enabled in the application worker. Memory observations do not prove collection
of cycles or bound an entire application's footprint.

Requirements: Linux with `/proc`, Node.js, Python 3.11+ (`PYTHON` selects the executable), Git,
and the same Rust toolchain for both engines. The pinned application was tested
here with Rust 1.99.1; the comparison adapter does not lower its dependencies'
MSRV to Temiya's 1.85. Use clean, pinned source checkouts and distinct Cargo
target directories. The builder uses omoikane's disk-space guard and requires
14 GiB initially, with a 12 GiB target limit and 2 GiB free-space floor.

```sh
git clone https://github.com/ieee0824/omoikane /tmp/omoikane-reference
git -C /tmp/omoikane-reference checkout --detach \
    007556adccb8fb4329236932ca8175f8e8d119d2
node scripts/extract-omoikane-workloads.mjs \
    /tmp/omoikane-reference /tmp/omoikane-inputs
CARGO_TARGET_DIR=/tmp/temiya-reference-target \
    cargo build --locked --release --example measure_workload
node scripts/build-omoikane-worker.mjs /tmp/omoikane-reference \
    /tmp/omoikane-worker /tmp/omoikane-reference-target
node scripts/check-workload-workers.mjs \
    /tmp/temiya-reference-target/release/examples/measure_workload \
    /tmp/omoikane-reference-target/release/omoikane-workload-worker
TEMIYA_REVISION=$(git rev-parse HEAD) node scripts/compare-omoikane-workloads.mjs \
    /tmp/omoikane-inputs \
    /tmp/temiya-reference-target/release/examples/measure_workload \
    /tmp/omoikane-worker/build-info.json 15 3 > /tmp/omoikane-comparison.json
```

The extraction/build output directories must be new. The build adapter copies
the pinned Cargo.lock, permits only the adapter's own new package, and checks
the resolved application/Boa features and Regress source before building.
`build-info.json` records source/binary/lock hashes and compiler information;
`cargo-metadata.json` and the adapter lock remain in the build directory.
Do not run builds or other benchmarks during the timed comparison.

The driver exits 2 if any workload fails but still writes the complete report.
Runs with fewer than 15 samples or three pairs are explicitly diagnostic; they
cannot qualify for its speed gate. All workload outcomes, samples, median/MAD,
startup/setup costs, memory observations, limits, and provenance are retained.
The original eleven-workload microbenchmark scope remains narrower than a full
application and omits async/RegExp/GC-specific suites and SpiderMonkey. #427
and the overall performance objective remain open until that work and the
required speed improvements are verified.

#### Recorded application baseline

[The first repeated comparison](../benchmarks/omoikane-comparison.json) uses
Temiya engine/worker revision `a994bae8414b961a8f0b8974d6ee03bc3d38a511`, the
pinned application above, Rust 1.99.1, release builds, and Linux ARM64. All
66 worker runs and 33 comparison pairs are retained. Boa completes all eleven
workloads; Temiya completes nine. The successful runs validate 960 results
including warmups. Every one of the 27 valid comparison pairs finds Temiya
slower beyond the recorded median/MAD threshold:

| Workload | Temiya time / Boa time across three pairs |
| --- | ---: |
| arith | 4.30–4.76× |
| prop-mono | 2.53–2.64× |
| prop-mega | 2.65–2.84× |
| call | 2.93–3.24× |
| closure-alloc | Not comparable: Temiya exceeds the 512 MiB RSS watchdog in all pairs |
| object-alloc | 2.06–2.17× |
| string-concat | 5.83–7.74× |
| array | 2.28–2.56× |
| primitive-string-property | 3.16–3.56× |
| primitive-string-method | Not comparable: Temiya's charCodeAt call fails in all pairs |
| proto-method | 2.69–2.93× |

Absolute times vary during the run; compare the paired measurements rather
than earlier standalone numbers. This result does **not** meet the performance
target. The memory failure is relevant to runtime ownership/GC work (#256,
#257, #313), and the missing method to #221. The complete records preserve the
failure statuses rather than dropping them or reporting a combined speedup.


#### String core follow-up

[The comparison after String core integration](../benchmarks/string-core-comparison.json)
uses engine/worker revision `13cea252ecf9e2f846b16f9e13a5602e2e52ed86`,
the same pinned application and unchanged eleven workloads, release Rust 1.99.1,
and Linux ARM64. There are 15 timed samples and one checked warmup per worker,
three alternating pairs per workload, and the same 512 MiB / 180-second limits.
All 66 runs and 33 pair outcomes are retained. Temiya completes 10/11
workloads in every pair. The 63 successful worker runs validate 1008 results
including warmups. The charCodeAt workload now returns its expected 999856;
closure allocation remains a separate ownership/GC task.

| Workload | Temiya time / Boa time across three pairs |
| --- | ---: |
| arith | 3.93–5.80× |
| prop-mono | 2.33–2.89× |
| prop-mega | 2.29–3.09× |
| call | 2.70–4.60× |
| closure-alloc | Not comparable: Temiya memory_limit |
| object-alloc | 1.91–2.47× |
| string-concat | 5.02–6.85× |
| array | 2.67–2.94× |
| primitive-string-property | 3.32–3.37× |
| primitive-string-method | 2.20–2.68× |
| proto-method | 1.88–2.93× |

This follow-up still does **not** meet the Boa performance target. It establishes
that the previously missing string method executes correctly; it is not an
optimization claim. Absolute times differ between measurement sessions, so the
two historical reports do not establish a controlled before/after speedup or
regression. Use the paired Boa comparisons within each report. SpiderMonkey
comparison and application-level validation remain outstanding. Legacy
`String.prototype.substr` is tracked separately in #454.


#### Function allocation retention follow-up

[The comparison after removing hidden self retention and deferring default
prototype values](../benchmarks/function-retention-comparison.json) uses engine/worker
revision `292a3760deb3f9d3208050350feb733c4aaf66fa`, the same pinned omoikane
embedding and unchanged eleven inputs, release Rust 1.99.1 and Linux ARM64.
There are 15 timed samples plus a checked warmup per worker and three alternating
pairs per workload, with the unchanged 512 MiB / 180-second limits. All 66 runs
and 33 pair outcomes are retained; 66 runs pass, validating 1056 results.

The original closure-allocation workload executes 300000 iterations and returns
715003. All three Temiya workers complete it, with observed peak HWM
3616–3624 KiB. The historical comparisons stopped this workload at
the memory limit; a failed earlier run cannot supply a before/after speed ratio.
These sampled peaks do not prove collection of arbitrary cycles.

| Workload | Temiya time / Boa time across three pairs |
| --- | ---: |
| arith | 4.39–4.66× |
| prop-mono | 2.59–2.66× |
| prop-mega | 2.65–2.79× |
| call | 3.09–3.15× |
| closure-alloc | 2.03–2.16× |
| object-alloc | 1.80–1.91× |
| string-concat | 5.80–6.21× |
| array | 2.55–2.70× |
| primitive-string-property | 3.12–3.89× |
| primitive-string-method | 2.32–2.48× |
| proto-method | 2.65–2.74× |

Both engines complete all eleven workloads correctly in every pair.
The Boa speed objective remains unmet. Compare paired samples inside this report;
absolute times across historical sessions do not establish controlled regression
or speedup. This change reduces allocation and unobservable retention; it does
not integrate tracing GC. The ownership probe still retains self-cycle,
two-object-cycle and escaped-prototype cases. Full GC and SpiderMonkey/application
comparisons remain separate outstanding requirements.

### Matched Temiya regression comparisons

To compare a runtime optimization against its measured predecessor, freeze the
predecessor worker **before** rebuilding. The helper checks its hash against the
prior comparison report and refuses to overwrite either destination:

```sh
node scripts/freeze-temiya-worker.mjs before-report.json \
    /tmp/before-target/release/examples/measure_workload \
    /tmp/frozen-before-worker /tmp/frozen-before-info.json
```

After building the changed Engine with the same Rust compiler, features and
release profile, use the original pinned workload directory:

```sh
TEMIYA_REVISION=$(git rev-parse HEAD) node scripts/compare-omoikane-workloads.mjs \
    --temiya-baseline /tmp/omoikane-inputs \
    /tmp/after-target/release/examples/measure_workload \
    /tmp/frozen-before-info.json 15 3 > after-before-report.json
```

This explicit mode validates both workers as Temiya and labels them before/after.
It keeps the same serial alternating order, per-pass numeric oracle validation,
watchdogs, timing boundary and median/MAD policy as the Boa comparison. The
`after_to_before_ratio` and `all_eleven_workloads_after_faster_beyond_threshold`
fields are Temiya regression evidence; they are not Boa superiority evidence.
Use the default mode separately for the application Boa target. Both modes
retain failures and exit 2 if any workload is invalid; low-sample diagnostics
cannot satisfy either mode's speed gate. A wrong hash or compiler is rejected
before executing the baseline.


#### Compact string storage results

The standalone [allocation observation](../benchmarks/compact-string-allocations.json)
compares the former Vec construction/clone operations with the public JsString
operations. In 10000 iterations after warmup, constructing and cloning the
10-unit name charCodeAt and cloning an 80-unit value require zero allocations.
The old operations respectively request 30000, 10000 and 10000 allocations
(including reallocations). New long construction still requires 20000 requests
for its Vec buffer and Arc header; it is not allocation-free. This probe records
requested bytes, not RSS or time, and is not linked into the timing worker.
Stable/MSRV produce identical observations. JsString remains 24 bytes and Value
32 bytes on the measured ARM64 target.

Two separate repeated comparisons use release Rust 1.99.1 and the original eleven
omoikane workloads with 15 timed samples, one checked warmup and three pairs:
[Temiya before/after](../benchmarks/compact-string-beforeafter.json) compares the
frozen predecessor `292a3760deb3f9d3208050350feb733c4aaf66fa` against
changed runtime `b9ddd75e7fef485db84d06b64f5bc793ba120020`;
[Temiya/Boa](../benchmarks/compact-string-boa.json) compares the changed runtime
against the same pinned actual application embedding. Each mode retains 66 runs
and 33 pairs and uses the unchanged 512 MiB/180-second guards. The modes run
serially, with engine order alternating within each mode.
The before/after mode validates 1056 results; the Boa mode validates 1056.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 0.88–0.91× | 4.21–4.39× |
| prop-mono | 0.78–0.87× | 2.37–2.39× |
| prop-mega | 0.88–0.94× | 2.33–2.49× |
| call | 0.77–0.81× | 2.53–2.57× |
| closure-alloc | 0.86–0.92× | 1.66–1.78× |
| object-alloc | 0.84–0.94× | 1.71–1.78× |
| string-concat | 0.61–0.67× | 3.78–3.91× |
| array | 0.79–0.87× | 2.27–2.41× |
| primitive-string-property | 0.82–0.87× | 2.76–2.80× |
| primitive-string-method | 0.81–0.94× | 1.61–2.07× |
| proto-method | 0.84–0.88× | 1.85–2.48× |

Twenty-two before/after pairs improve beyond the existing threshold; eleven are
within the threshold. No pair regresses beyond it. All 33 Boa pairs find Temiya
slower beyond the threshold. A before/after improvement establishes a change
against the prior Temiya worker; it cannot satisfy the Boa objective.
The recorded Boa speed gate is false.
These eleven microbenchmarks do not establish full application compatibility or
SpiderMonkey superiority. Identifier atoms, ropes and real Engine GC remain
outstanding; this change only replaces immutable leaf string storage.


## Bounded string concatenation

The [allocation observations](../benchmarks/string-concat-allocations.json)
include final flattening, and remain separate from throughput measurements.

The [ARM64 before/after](../benchmarks/string-concat-beforeafter.json) compares
frozen predecessor `b9ddd75e7fef485db84d06b64f5bc793ba120020` with runtime
`4cf89c1d555ceefeecb2b53882fdb077440da012`. The separate
[ARM64 Boa comparison](../benchmarks/string-concat-boa.json) uses the same pinned
actual application embedding. Both modes use Rust 1.99.1 release builds with
15 samples and one validated warmup, three alternating pairs, 66 successful
worker runs and 1056 validated results each. The modes run serially after all
builds, using all eleven original inputs and the existing 512 MiB/180-second
guards.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 0.98–1.04× | 4.38–4.58× |
| prop-mono | 0.95–1.15× | 2.14–2.43× |
| prop-mega | 0.98–1.37× | 2.33–3.14× |
| call | 0.82–1.05× | 2.54–2.88× |
| closure-alloc | 1.01–1.02× | 1.47–1.50× |
| object-alloc | 0.77–1.04× | 1.58–1.94× |
| string-concat | 0.88–1.05× | 3.28–3.88× |
| array | 0.72–0.97× | 2.27–2.37× |
| primitive-string-property | 0.89–0.96× | 2.69–2.85× |
| primitive-string-method | 0.96–0.99× | 1.87–2.19× |
| proto-method | 0.87–1.01× | 2.04–2.19× |

All 33 ARM64 before/after pairs are within their noise thresholds; this run
does not prove a throughput improvement or the absence of regression. The
standalone allocation reduction cannot establish an Engine speedup.
Boa outcomes: {"Temiya slower beyond threshold":31,"within noise threshold":2}.
The all-eleven before/after speed gate is false;
the all-eleven Boa speed gate is false.

A [focused prop-mega diagnostic](../benchmarks/string-concat-cpu-followup.json)
checks one workload again with the same binaries, 15 samples and three alternating
pairs. Whole-process user+system CPU-time ratios are 0.93, 1.04 and 0.98; timed
wall-median ratios are 0.93, 1.07 and 0.98. Process CPU totals include startup,
setup, warmup and final reporting and are not substitutes for timed medians.
The focused diagnostic has a 180-second timeout but no RSS watchdog. It does
not prove a speedup or absence of regression and cannot pass an all-eleven gate.
The original observations remain in the complete before/after report.

The [opt-in hosted comparison](omoikane-benchmark-ci.md) repeats both modes
on a separate runner. Results from different architectures must not be pooled.
These eleven microbenchmarks do not establish full application compatibility
or SpiderMonkey superiority. Identifier atoms, balanced ropes and real runtime
GC remain open; #415 and #427 are not complete.


## Hosted x86_64 bounded-concatenation comparison

[Run 40361665749](https://github.com/ieee0824/temiya/actions/runs/40361665749)
completed on Ubuntu 24.04, AMD EPYC 7763, four logical CPUs. The
[provenance record](../benchmarks/string-concat-hosted-provenance.json) retains
build identity and the original artifact reference. Both
[before/after](../benchmarks/string-concat-hosted-beforeafter.json) and
[Boa](../benchmarks/string-concat-hosted-boa.json) use the original eleven
workloads, 15 timed samples, one checked warmup and three alternating pairs.
Each mode has 66 successful runs and 1056 validated values. They run serially
after all builds, with the same compiler and existing 512 MiB/180-second guards.

The measured revision is `ee1c0bcf0ca3533382eef0269625104a1a247ece`;
the baseline is `82c572d0e42c8f97deccb349a02095e15b2c8af1`. The measured
runtime is identical to code revision `4cf89c1d555ceefeecb2b53882fdb077440da012`.
Later ARM64 data commits do not modify executable source, worker scripts,
dependencies or the measurement workflow. Architectures are reported separately.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 0.98–1.00× | 3.33–3.42× |
| prop-mono | 0.94–0.96× | 2.42–2.49× |
| prop-mega | 0.97–0.98× | 2.30–2.37× |
| call | 0.93–1.00× | 2.34–2.36× |
| closure-alloc | 0.93–0.95× | 1.82–1.90× |
| object-alloc | 0.96–0.98× | 1.79–1.85× |
| string-concat | 0.86–0.88× | 3.06–3.12× |
| array | 0.94–0.96× | 2.24–2.40× |
| primitive-string-property | 0.99–1.00× | 2.70–2.84× |
| primitive-string-method | 1.00–1.03× | 1.95–2.00× |
| proto-method | 0.95–0.97× | 2.14–2.28× |

All three string-concat pairs improve beyond the 10% threshold: elapsed time
is 12–14% lower than the prior Temiya build. The other 30 pairs are within
the threshold; no pair regresses beyond it. This establishes the observed
string-concat improvement on this runner, rather than a general engine speedup.
All 33 Boa pairs still find Temiya slower beyond the threshold. Both all-eleven
speed gates remain false. The allocation reduction and successful CI do not
satisfy the Boa goal or complete #415/#427.


## Hosted x86_64 merged-main comparison

[Run 40428633049](https://github.com/ieee0824/temiya/actions/runs/40428633049)
completed on Ubuntu 24.04, Intel Xeon Platinum 8573C, four logical CPUs. The
[provenance record](../benchmarks/combined-main-hosted-provenance.json) retains
build identity and the original artifact reference. Both
[before/after](../benchmarks/combined-main-hosted-beforeafter.json) and
[Boa](../benchmarks/combined-main-hosted-boa.json) use the original eleven
workloads, 15 timed samples, one checked warmup and three alternating pairs.
Each mode has 66 successful runs and 1056 validated values. They run serially
after all builds, with the same compiler and existing 512 MiB/180-second guards.

The measured revision is `190b01f5de6ae323e0149ea4c164bdfa5b4f37b6`;
the baseline is `c34c797cbb717b96efb3c952c9655baa38637c41`. The measured
runtime is identical to code revision `190b01f5de6ae323e0149ea4c164bdfa5b4f37b6`.
Later result-only commits do not modify executable source, worker scripts,
dependencies or the measurement workflow. Architectures are reported separately.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 0.98–1.00× | 3.24–4.20× |
| prop-mono | 1.00–1.02× | 2.63–2.69× |
| prop-mega | 1.01–1.05× | 2.24–2.61× |
| call | 0.99–1.02× | 2.76–2.82× |
| closure-alloc | 1.02–1.03× | 1.80–1.90× |
| object-alloc | 1.04–1.06× | 1.75–1.90× |
| string-concat | 0.95–0.96× | 3.82–3.83× |
| array | 1.02–1.03× | 2.53–2.58× |
| primitive-string-property | 0.98–1.00× | 3.22–3.31× |
| primitive-string-method | 1.00–1.01× | 1.79–2.17× |
| proto-method | 1.00–1.03× | 2.13–2.45× |

All 33 before/after pairs remain within the noise threshold; no throughput
speedup or regression beyond that threshold is established on this runner.
All 33 Boa pairs still find Temiya slower beyond the threshold. Both all-eleven
speed gates remain false. Merged-main integration does not satisfy the Boa goal
or complete #307/#427.

### Earlier allocation observation (#475)

The frozen numeric-only report observed one object-alloc pair at 1.103591×
baseline time, while its other pairs were 0.952615× and 0.967431×. That observation
is retained in the numeric reports. The combined-main report above observes
1.049294×, 1.036001× and 1.058231× for the same workload; all three remain within
the unchanged 0.1 threshold. The earlier threshold crossing is not reproduced
in this combined-source comparison. All three new ratios remain above one, so
this is not evidence of an allocation speedup.

The numeric report used an AMD EPYC 9V45 and the earlier isolated numeric source;
this combined report uses an Intel Xeon Platinum 8573C and includes all merged
#469–#474 changes. The sources and CPUs differ, so these observations do not
identify the earlier delay's cause or prove that an isolated regression was
fixed. Both reports, full samples, median/MAD and classifications remain available.
The full Boa speed objective remains unmet. The subsequent native prototype PR
#476 has its own source-specific comparison and is not included in revision 190b01f.
