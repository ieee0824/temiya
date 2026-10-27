# Typed Number dispatch observations

These records retain the initial typed-dispatch candidate (`b8b0f6b0`), direct
Number binding-copy candidate (`4fb82dbc`), and inline candidate (`37e09968`).
Hosted x64 qualification of source 37e09968 completed on AMD EPYC 9V45 and 9V74.
This package retains every observed cohort. Intel issue #510 remains unmet. This is evidence for individual recorded
inputs and machines, not whole-application or SpiderMonkey superiority.

Every original-eleven report uses pinned actual omoikane
`007556adccb8fb4329236932ca8175f8e8d119d2`, unchanged fixture v2, fifteen measured
samples plus a checked warmup, three alternating pairs, serial workers,
512 MiB RSS and 180 seconds. All numerical results must be correct. The fixed
noise rule is max(0.1, 3*(MADa/mediana + MADb/medianb)); noise is not a win.
The actual application sandbox is recorded and is not replaced by upstream Boa.
Invocation parse/evaluate is timed; Temiya drops the per-invocation Script handle
before stopping its timer, while actual Boa uses its complete pinned public eval
call. Setup/startup/warmup are separate.

| Directory | Source and machine | Interpretation |
| --- | --- | --- |
| initial-arm | b8b0f6b0 / ARM64, CPU model unavailable | Initial slower prop-mono pair retained |
| initial-amd9v45 | b8b0f6b0 / AMD EPYC 9V45 | Run 40547572815, artifact 12753989149 |
| initial-amd9v74 | b8b0f6b0 / AMD EPYC 9V74 | Same run, artifact 12754763251 |
| final-arm | 4fb82dbc / ARM64 | Number-copy cohort, including separate Temiya-only async control |
| final-intel | 4fb82dbc / Intel Xeon 6973P-C | Run 40555924150, artifact 12760240625; arithmetic gate unmet |
| inline-arm | 37e09968 / ARM64 | Full controls; same-executable control has three slower labels |
| inline-hosted-copy1 | 37e09968 / AMD EPYC 9V45 | Run 40562000380, artifact 12762482172 |
| inline-hosted-copy2 | 37e09968 / AMD EPYC 9V74 | Same run, artifact 12763052819 |
| initial-async | b8b0f6b0 / recorded x64 host | Run 40548626206, artifact 12753971934 |
| final-async | 4fb82dbc / recorded x64 host | Run 40555188016, artifact 12757910855 |
| inline-async | 37e09968 / recorded x64 host | Run 40560188183, artifact 12762125493 |

“final” in directory names denotes the Number-copy cohort and does not designate
completion of the performance objective. Exact full revisions, compiler strings,
worker/binary/source hashes and machine records are retained inside each report
and in provenance.json. Local synchronous baseline binaries honestly retain
revision def9649b; git diff verified their runtime, worker and Cargo inputs equal
baseline main fdd4ac6d. Same-executable faster/slower labels are variability, not
source changes. Do not delete unfavorable pairs or infer before/after effects
across different machines or separate cohorts.

The three authored async inputs are separate from the original eleven. Their
actual scheduling, checkpoint and completion evaluation are timed. Every actual
Boa async report has fourteen positive/negative native adapter checks and 288
checked values. The inline report has six faster and three noise decisions;
it does not meet an all-nine-faster gate. The additional local async before/after
report uses two Temiya workers, not Boa. Its archived adapter is a verbatim source
snapshot with original temporary import paths, not a portable invocation entrypoint.

The six profile directories contain reduced (20,000 iterations) and original
(2,000,000 iterations) Callgrind diagnostics with independently checked oracles.
Their whole-process instruction references include startup, setup, warmup and one
sample; they are not hardware cycles or uninstrumented speed acceptance. Profile
workflow-only revisions are related to runtime revisions by recorded git diffs.
Raw profiles and annotations use lossless gzip to avoid large generated diffs;
inventory.json includes compressed and uncompressed hashes. For inspection:

```sh
gzip -dk benchmarks/typed-number/*/*.gz
node benchmarks/typed-number/verify-integrity.mjs benchmarks/typed-number
node benchmarks/typed-number/verify-recorded-timing.mjs benchmarks/typed-number
```

Integrity verification checks stored bytes and lossless decompression. Timing
verification independently checks exact suite coverage, numerical oracles,
validated samples, alternating order, medians/MAD, ratios, thresholds and gates.
These do not claim to rerun original native binaries or reestablish source/build
provenance; those checks were performed before archiving and their records remain.

Hardware-only directories record exact requested CPU models before building.
Unavailable hardware skips measurements and is never a speed/correctness pass.
All availability attempts, including unavailable Ubuntu 24.04/22.04 hosts, are
retained. The actual matched Intel record is included alongside its complete
measurements. AMD/ARM success does not substitute for the Intel requirement.

Memory observations are sampled whole-process HWM including setup/warmup, not
retained heap or allocation counts. Reported embedding startup excludes process
launch; setup is not a dedicated parse-only suite. RegExp/GC and other groups
under #427 remain unfinished.

The local actual-Boa synchronous binary was built from the preserved historical
adapter source in local-sync-worker-source.rs (SHA256 031ee68a...), whose rustfmt
output is byte-for-byte identical to the current adapter (efba2d69...). Its
original build note refers to the initial diagnostic; the exact frozen binary
was subsequently used for the complete repeated ARM reports. No scripted rebuild
is claimed for those cohorts. Actual pinned application, lock hashes, feature
sets, compiler, sandbox and numerical checks remain recorded.
