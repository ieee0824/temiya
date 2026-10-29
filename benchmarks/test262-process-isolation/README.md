# Test262 process isolation validation (in progress)

Issue #150 runs every executable variant in its own Linux worker, with a finite
wall deadline, pre-exec address-space/stack limits, RSS supervision and a zero
core-file limit. Fatal process/resource failures remain failures even when the
source has a language negative expectation. Normal RangeError guards retain
their language semantics. Worker exits and applied resource bounds are reported.

Source fb07e6358ff6ab8d62b7e1f8f3aa7e3e1124472a has:

- 705 passing native tests and 705 passing Rust 1.85.0 tests, each with 26 ignored.
- Successful all-target/all-feature Clippy with warnings denied, formatting and
  C ABI header/example/export checks. Its hosted CI passes all four jobs.
- Six real CLI regressions covering synchronous infinite loops, global and
  pending-job isolation, real allocation and expression-stack exhaustion,
  ordinary RangeError negatives, SIGABRT/SIGKILL and subsequent successful tests.
  The signal tests read the actual address-space, stack and core-file limits
  from `/proc`. SIGKILL alone is not reported as OOM.
- 247 original pinned Test262 variants pass across nine retained CLI reports;
  none fails or skips. Original harness/dependencies are unchanged. These are
  selected regressions, not whole-corpus or full language qualification.

The first SIGSEGV-injection test did not terminate the worker and hit its wall
limit. It is preserved as a failed test attempt; deliberate crash tests now use
SIGABRT/SIGKILL. The initial exact-manifest-string JIT policy failure and the
failed offline full-metadata attempt are also preserved. The replacement policy
queries Cargo's actual root features with offline `cargo tree`: JIT remains
optional while libc also supplies OS resource limits. Nayuta v0.3.2's candidates
were reviewed; no confirmed Nayuta defect was found.

`node benchmarks/test262-process-isolation/verify.mjs` recomputes log counts,
original result totals, compressed/raw hashes, source input hashes and hosted
CI success. Performance qualification and integration after #149 are pending.
The bounds cover disposable Linux workers; parent corpus metadata loading,
non-Linux resource enforcement and security sandboxing are not qualified.
