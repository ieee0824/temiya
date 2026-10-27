# Native callable initialization

PR #480 initializes complete own name/length descriptors on a fresh ordinary
native callable, without two generic DefineOwnProperty operations. The private
constructor seeds the prototype, callback, and two-entry property vector before
the object is exposed. Attributes and insertion order are unchanged; subsequent
mutations use the normal object methods. Callback ownership and the public
Realm factory remain unchanged.

## Allocation profiles

The [release probe](../examples/realm_startup_probe.rs) compares raw native
callables, short and long Unicode names, generic descriptor definition, and
public Engine/Realm initialization. Counters are disabled for timing samples.
Each allocation profile uses 128 warmups; counts below exclude those warmups.

| Operation | Count | Before requests / bytes | After requests / bytes |
| --- | --- | --- | --- |
| Raw callable, no metadata | 1000 | 2000 / 360000 | 2000 / 360000 |
| Short-name native factory | 1000 | 3000 / 744000 | 3000 / 552000 |
| Long Unicode native factory | 1000 | 5000 / 888000 | 5000 / 696000 |
| Engine plus Realm creation | 10 | 6440 / 1325540 | 6440 / 1166180 |

All three process repetitions have identical allocation profiles. Each metadata
case checks prototype, own-key order, values and descriptor attributes against
generic property definition. Requested bytes decrease by 192 per native method
and 15936 per Realm (about 12%). Allocation request counts do not decrease.
Requested bytes are neither retained heap nor RSS.

## Separate startup measurements

Both isolated release targets used Rust 1.99.1 on local Linux ARM64, four logical
CPUs (CPU model unavailable), with all builds finished before timing. Runtime
9c9e6c59ce675f2c81e788230e4871c61fb1b5d8 is compared against main
 d2d12825f349cd37933d65c0dbde96b29971c105. The same new probe was added to the
baseline solely for measurement; its runtime source stayed unchanged.

- [Repeated startup and allocation data](../benchmarks/native-function-initialization-repeated.json):
  three alternating pairs, 50 repeated Engine/Realm creations per process after
  allocation-profile warmups. After/before median ratios are 0.853, 0.851, 0.869.
- [First startup per fresh process](../benchmarks/native-function-initialization-cold.json):
  three groups with 25 new processes per worker, alternating launch order.
  Uses the unchanged measure_workload example, empty setup and a checked 42
  invocation. Only startup_ns is compared; this is a separate diagnostic and
  does not substitute for the original eleven invocation workloads.
  After/before median ratios are 0.877, 0.869, 0.902.
- [Build identities, allocation details and verification](../benchmarks/native-function-initialization-provenance.json).

All six repeated-profile processes and 150 first-startup processes completed
successfully. Every median/MAD, ratio and threshold was independently recomputed.
All six startup comparisons are within the original noise rule
max(0.10, 3 * (after MAD/median + before MAD/median)); both startup gates are
false. Lower medians do not establish a confirmed latency improvement.
Issue #478 remains open, including its startup-improvement acceptance condition.

Short-lived startup probes can finish before RSS polling. Zero in the repeated
recording is an unobserved sentinel; null in the first-startup recording means
unavailable. Neither proves a zero HWM or a measured RSS bound. Original eleven
invocation comparisons keep their separate 512 MiB / 180 second guards.

## Reproduction and compatibility

Build examples/realm_startup_probe.rs with --locked --release in two isolated
worktrees/targets at the revisions above and the same compiler. The baseline
needs the identical probe and existing examples/support/allocations.rs. Run each
worker with argument 50 in alternating order, retaining its complete output.
The [repeated recording script](../benchmarks/recordings/native-function-initialization-repeated.mjs)
and [first-startup recording script](../benchmarks/recordings/native-function-initialization-cold.mjs)
archive the exact recording logic and original local paths. Set up those paths
or adjust them for a new recording; retain the published observations separately.
The first-startup fixture uses exclusive creation and will refuse an existing
file. These scripts record experiments; they are not CI performance gates.

The exact source passed the four CI and three security jobs for stable/MSRV,
Clippy, C ABI, Miri, ASan and dependency auditing. Nine native prototype/lifetime
regressions also passed locally in release. The [original numeric audit](../test262/native-function-initialization-numeric.json)
retains all 62 variants and fixture blob identities: 58 pass, four known failures,
zero skips, and every outcome unchanged from the existing main audit. Its full
opt-in gate still exits 101; failures and ignored tests are not passing coverage.
The independent hosted eleven-workload comparison is still running. No overall
Boa superiority or whole-engine conformance is established.
