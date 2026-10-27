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

No successful native adapter run or performance result is recorded yet for
this implementation. Node syntax checks alone do not qualify it for merging.

Pinned API source:
[evaluate/dateNow](https://github.com/mozilla-firefox/firefox/blob/5a78f4525f81f7323c2c044965096723318af6e6/js/src/shell/js.cpp),
[file input](https://github.com/mozilla-firefox/firefox/blob/5a78f4525f81f7323c2c044965096723318af6e6/js/src/shell/OSObject.cpp).
