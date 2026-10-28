# Const VM and fresh function initialization, source 41327b9

The original eleven pinned omoikane workloads retain their bodies, counts,
oracles, 512 MiB/180-second watchdog, 15 samples and three alternating pairs.
Both frozen release workers use Rust 1.99.1 and guest CPU0 on local ARM64;
Codex/code-mode threads were on guest CPUs1–3 during competitive timing.
This is guest affinity, not host physical-core isolation. Agent affinity was
restored afterward. No local build, test or profiler overlapped timing.

`boa.json`: all66 workers/1056 values are correct. All33 comparisons pass the
unchanged max(10%, three times summed relative MAD) improvement gate. Time
ratios range0.396–0.803 against the pinned application's actual modified Boa
through JsRuntime/Sandbox. Independent recomputation is in `provenance.json`.
This result is bounded to the recorded machine and conditions. Hosted results
are still pending. The unfinished413 full Miri run was later cancelled when
the measured efd candidate superseded it; see `security-superseded-reason.json`.
Its partial successes are not a full119 pass. Earlier-source passes do not
establish completion for a later implementation.

`beforeafter.json` retains the comparison with c74: three faster,28 noise,
two slower. Closure takes0.781–0.793 times the old duration in all three pairs.
Prop-mono pair2 and prop-mega pair1 regress; the change is not regression-free.
No pair/sample or unfavorable result was discarded.

Local native655 tests passed,21 ignored; Clippy and selected fixed Test262
checks passed. Four exact integration Miri checks passed with strict provenance
and symbolic alignment. `ownership-probe.json` releases all seven fixtures
after retaining and using the original/cloned handles. CI40749946254 passed
stable/MSRV, formatting/Clippy and C ABI checks at the exact measured source.

`runtime-profile.json` is a separate instrumented single pass of the same11
inputs. All oracles match, all final live/rooted counts are zero, and root
registration/removal calls balance. Managed-cell counts, pause and remaining
evaluation time are diagnostic; these counters/timers are absent from the
competitive worker. They do not count all Rust allocations or facade clones.

The target audit confirms the latest observed omoikane Boa/Regress trees and
dependency manifests match the measured pin. Its embedding tree differs;
no whole-application superiority or compatibility claim follows.
