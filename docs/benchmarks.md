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
additional target. Neither has been demonstrated. The current internal suite
is insufficient evidence for either claim.

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
