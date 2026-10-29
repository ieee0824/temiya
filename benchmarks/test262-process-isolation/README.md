# Test262 process isolation validation

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
CI success. Main's module-fixture changes were integrated without changing any
of the 83 validated source/fixture inputs, preserving the original source commit
in the branch history.
The bounds cover disposable Linux workers; parent corpus metadata loading,
non-Linux resource enforcement and security sandboxing are not qualified.

## Performance records

The new worker is independently measured in twelve reports in
[`performance/`](performance/). Each environment has candidate/baseline,
candidate/actual Boa, baseline/actual Boa and identical-executable controls.
The original eleven bodies/counts/oracles, 15 samples, three alternating pairs,
180s/512MiB guards and statistical thresholds are unchanged. Each report passes
66 worker executions and 1056 checked values; all samples and outcomes remain.

| Environment | Candidate vs actual Boa | Candidate vs baseline | Identical executable control |
| --- | --- | --- | --- |
| ARM64 | 31 faster / 2 noise | 30 noise / 2 slower / 1 faster | 32 noise / 1 slower |
| x64 copy 1 | 33 faster | 33 noise | 33 noise |
| x64 copy 2 | 33 faster | 33 noise | 30 noise / 2 slower / 1 faster |

ARM closure-alloc pairs0/1 are slower against the previous worker (ratios
1.119901 and 1.126095, thresholds 0.105736 and 0.10). These remain observations;
variability in the identical-executable controls does not establish their cause
or eliminate the flags. No whole-engine or all-environment speed claim follows.
The two x64 candidate/Boa reports qualify the fixed eleven-workload target on
their recorded hosts.

The ARM baseline remains labelled with its actual build source d25d974; hosted
baseline de290ef has identical baseline inputs, verified by hashes. The frozen
ELF is preserved as 8f9c59d; candidate ARM ELF is 38c8513. A source-equivalence
proof does not relabel a different build or manufacture a measurement.
Run `node benchmarks/test262-process-isolation/performance/verify.mjs` for all
twelve reports, feature/lock/compiler/binary/source identities and functional
validation together. #531 remains the separate earlier module-fixture
primitive-string-method regression; these records do not claim an isolated fix.
