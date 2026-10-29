# Managed packed numeric elements

The files in this directory describe the first implementation at source
25e46c7608366603b68504d73dc3ba78a2acbfc8. Later trials are recorded separately:

- `experiments/cow-25e`: original owned-descriptor reads, including all adverse
  ARM64/hosted results, controls, source identities and completed security proof.
- `experiments/view-6fb`: smaller numeric/borrowed-descriptor read view at source
  6fb3160be6d966b7b0c35fb7b5da06ddb33b3583, with separate validation and timings.
- `experiments/fused-dd7`: guarded numeric binding writes and direct indexed
  values at dd7df4455bbcfcf8764464ff2185eb4650f1372a; all33 actual-Boa pairs
  qualify on ARM64, AMD7763 and Intel8370C with no before/after regression.
  Native/MSRV/quality and119 exact library Miri/all19 security jobs pass.

Audit retained original-eleven reports with
`node scripts/audit-original-workload-records.mjs REPORT.json [...]`.
The auditor recomputes all values, sample statistics, alternating orders and
fixed gates. An audit success verifies the recorded outcome, including noise
and regressions; it does not turn a report into a speed qualification.
For hosted cohorts, also run
`node scripts/audit-omoikane-hosted-identities.mjs COHORT_DIR MEASURED_SHA BASELINE_SHA`
to check compiler/adapter/features/locks and all four reports' worker/control
identities. Neither auditor requires a favorable timing outcome.

This is a new trial on the integrated tracing runtime, not the earlier pre-GC
packed-number trial. Contiguous default numeric descriptors use8-byte values;
other values/attributes/holes/sparse entries restore full descriptors. Numeric
slots have no GC edges; descriptor-backed entries retain full strong tracing.

Native661 pass/21 ignored, Clippy, two exact public integration Miri tests and
selected pinned array Test262 regressions pass. The allocation-only probe
requests585216 bytes before versus65024 after (7 requests each), validates
sum8386560 for4096 entries and releases all cells. The same probe source was
temporarily added to the reference checkout; the runtime/manifests were kept
unchanged and the probe removed before freezing the clean timing worker.

The initial Cow trial completed its comparisons and security checks but did not
meet the performance objective. Its unfavorable reports remain preserved in
`experiments/cow-25e`. The top-level provenance's pending list describes the
original recording snapshot; subsequent results are in the experiment folders.
The view trial qualifies all33 actual-Boa pairs on ARM64, but hosted cohorts have
4 and5 before/after regressions and one actual-Boa noise decision. Full runtime
security passes119 exact Miri tests and all19 jobs; the view is still not accepted
for merge. The x64 diagnostic localizes extra work in general binding writes.
Keep original11 counts/oracles/
15samples/3pairs/512MiB/180seconds/10%-MAD gates and all observations, including
unfavorable/noise outcomes. Allocation bytes are not timing or peak memory.
