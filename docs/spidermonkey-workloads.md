# Native SpiderMonkey workload comparison

The opt-in `spidermonkey-benchmark.yml` workflow downloads a successful pinned
shell artifact, verifies its archive and executable hashes, builds the current
release Temiya worker, and then runs the original eleven omoikane workloads.
All builds and adapter checks finish before serial timing begins. Both shell
modes run separately with 15 samples, one checked warmup and three alternating
pairs. The original independent oracles, source hashes, memory/time watchdogs
and median/MAD decision rule remain in force. A failing worker is never a win.

The shell adapter uses the pinned native `evaluate` entry point for each
invocation, with source strings rather than cached bytecode. Setup is excluded.
Its sub-millisecond `dateNow` wall clock is converted to integer nanoseconds;
non-positive intervals fail. This clock is not claimed to be monotonic or to
have nanosecond precision. Native process startup is excluded and reported as
unmeasured, rather than substituting the setup duration. Script reclamation and
GC follow the shell's normal behavior; no forced GC is inserted in either
engine's timed interval.

Interpreter mode verifies that all three tier settings are disabled. Default
mode records enabled settings without claiming that workload code reached a
particular JIT tier. The actual C++/Rust shell compiler and current Temiya Rust
compiler are recorded separately. A same-Rust-compiler requirement remains in
force for the existing Temiya/Boa and Temiya before/after comparisons.

`check-spidermonkey-worker.mjs` exercises the native adapter in both modes with
ordinary and negative-zero results, rejected wrong results (including signed
zero), syntax errors and runtime errors. These are adapter checks, not full
Test262 conformance. This suite adds the original eleven shell comparisons;
async/RegExp/GC-specific workloads remain separate work under #427. The pinned
omoikane public-embedding comparison remains the mandatory Boa reference.

The actual native adapter passed all 12 checks in
[run 40533723077](https://github.com/ieee0824/temiya/actions/runs/40533723077)
at tool revision 00d2ce517dae86d4d4f22a602da228c049423d35. Both mode reports
contain 66 successful workers and 1,056 checked results (132 workers / 2,112
results total). All medians, MADs, ratios, thresholds, alternating order, full
coverage, input/oracle identity, backend selection and shell/adapter hashes were
independently checked. All 33 comparisons in each mode are slower for Temiya
beyond the fixed noise threshold; both all-eleven speed gates are false.

The runner was Linux x64 on AMD EPYC 9V45 (four logical CPUs). Temiya used
rustc 1.99.1 release with no features; the native shell build used clang 22.1.8
and rustc 1.99.1. Every Temiya setup and invocation actually selected Bytecode
through the normal Auto Script API. Ratios below are Temiya median elapsed
time divided by SpiderMonkey median elapsed time, with the range across all
three alternating pairs. A value above one means Temiya took longer.

| Workload | JIT-disabled mode | Default settings, workload JIT unproven |
|---|---:|---:|
| arith | 9.43–10.45 | 56.81–58.13 |
| prop-mono | 6.56–7.28 | 310.21–348.23 |
| prop-mega | 7.21–7.69 | 83.53–85.49 |
| call | 5.08–5.21 | 100.16–101.99 |
| closure-alloc | 6.18–6.50 | 186.86–198.45 |
| object-alloc | 6.07–6.19 | 175.96–177.35 |
| string-concat | 6.27–6.78 | 52.07–60.40 |
| array | 6.17–6.27 | 91.03–93.23 |
| primitive-string-property | 9.48–9.86 | 52.33–53.28 |
| primitive-string-method | 7.59–7.85 | 113.26–114.76 |
| proto-method | 4.75–4.93 | 141.20–142.79 |

Sampled worker peak HWM ranged 4,888–5,292 KiB for Temiya and 24,536–57,196 KiB
for SpiderMonkey in interpreter mode; default mode ranged 4,852–5,300 KiB and
24,516–55,048 KiB respectively. These are process observations under the same
512 MiB watchdog, not live-heap, collector completeness or application-wide
memory claims. No failure, timeout or skipped pass is used as a speedup.

[Interpreter raw data](../benchmarks/spidermonkey/interpreter.json),
[default raw data](../benchmarks/spidermonkey/default.json),
[native adapter checks](../benchmarks/spidermonkey/worker-checks.json) and
[build/run provenance](../benchmarks/spidermonkey/provenance.json) preserve
the negative result. The previously established actual-omoikane-Boa advantage
on these eleven inputs is a separate comparison; it does not imply a
SpiderMonkey advantage.

Pinned API source:
[evaluate/dateNow](https://github.com/mozilla-firefox/firefox/blob/5a78f4525f81f7323c2c044965096723318af6e6/js/src/shell/js.cpp),
[file input](https://github.com/mozilla-firefox/firefox/blob/5a78f4525f81f7323c2c044965096723318af6e6/js/src/shell/OSObject.cpp).
