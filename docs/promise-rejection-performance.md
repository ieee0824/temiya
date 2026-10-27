# Promise rejection-value follow-up

The correctness fix retains arbitrary JavaScript reasons in reactions,
thenable rejection and host hooks, and marks consumed settled await results
handled. It makes no execution-speed improvement claim. The measured source is
`def9649b99ccf382aa96117871b6107e05fe8456`.

All six original-eleven reports completed correctly: 396 workers, 6,336 checked
results and 198 independently recomputed pair decisions. Each report uses the
unchanged original inputs, 15 samples plus a checked warmup, three alternating
pairs and the existing 512 MiB / 180-second watchdog and median/MAD rule.
Startup/setup/warmup remain outside invocation timing. All Temiya setups and
invocations actually selected Bytecode through the default Auto Script API.

| Dataset | Before/after faster / within noise / slower | Actual Boa faster / within noise / slower |
|---|---:|---:|
| ARM64 initial | 2 / 30 / 1 | 33 / 0 / 0 |
| ARM64 complete repeat | 2 / 30 / 1 | Not rerun |
| ARM64 identical-binary control | 1 / 30 / 2 | Not applicable |
| Hosted Intel Xeon 6973P-C | 0 / 31 / 2 | 25 / 5 / 3 |

The initial ARM slowdown was prop-mega pair 0 (ratio 1.3820); the complete repeat
did not reproduce that slowdown and instead flagged call pair 1 (1.1184). The
control used exactly the same frozen executable and SHA for both worker labels,
yet flagged call pair 1 (1.1239), primitive-string-property pair 2 (1.2391) and
string-concat pair 1 as faster (0.8666). These control differences cannot be
attributed to a source change. They demonstrate process/environment variability
outside the within-worker MAD; they do not erase any original negative result
or prove that all candidate regressions are absent. The captured cgroup
statistics show no quota throttling, without identifying the cause of variance.

The new Intel result is a genuine unmet comparison target: arithmetic is slower
than actual Boa in all three pairs, with elapsed-time ratios 1.1280, 1.1269 and
1.1205 against threshold 0.1. Its paired candidate/baseline arithmetic results
are all within the existing noise threshold. This does not directly establish
the baseline/Boa ratio; that needs its own paired measurement. The previous
all-eleven hosted success used AMD EPYC 9V74 rather than this Intel CPU. No
cross-machine ratio is used to establish a regression, improvement or cause.
The Intel target is tracked in [#510](https://github.com/ieee0824/temiya/issues/510).

The [hosted run](https://github.com/ieee0824/temiya/actions/runs/40536890790) used
rustc 1.99.1 and actual main `24b027f` as baseline. ARM used the previously frozen
`351de1c` Auto worker; its runtime, worker source and dependencies were checked
identical to main `24b027f`, and its original compiler metadata matched rustc
1.99.1. The repeat and control reused the same frozen workers without rebuilding.

[Provenance and binary/compiler identities](../benchmarks/promise-rejection/provenance.json)
link all reports. Raw reports retain the
[initial ARM before/after](../benchmarks/promise-rejection/arm-initial-beforeafter.json),
[ARM Boa comparison](../benchmarks/promise-rejection/arm-initial-boa.json),
[complete ARM repeat](../benchmarks/promise-rejection/arm-repeat-beforeafter.json),
[same-executable control](../benchmarks/promise-rejection/arm-same-binary-control-beforeafter.json),
[Intel before/after](../benchmarks/promise-rejection/hosted-intel-beforeafter.json) and
[Intel Boa comparison](../benchmarks/promise-rejection/hosted-intel-boa.json).
The current evidence preserves the measured ARM advantage but does not establish
a universal Boa advantage or complete #427's remaining suites.
