# Numeric/borrowed descriptor view, source6fb3160

Runtime source: `6fb3160be6d966b7b0c35fb7b5da06ddb33b3583`.
Reference main: `c7c4f265c4002de1d90223eccaa64aaafdada52a`.
The runtime replaces full owned read descriptors with a small numeric or
borrowed descriptor view. Ordinary flags and managed owner checks are retained.

| ARM64 CPU0 comparison | Faster beyond gate | Within gate | Slower beyond gate |
| --- | ---: | ---: | ---: |
| Candidate / reference Temiya | 0 | 33 | 0 |
| Candidate / actual modified Boa | 33 | 0 | 0 |

Each report has66 successful workers,1056 checked values and33 independently
recomputed decisions. Inputs are the original eleven,15 samples plus a checked
warmup,3 alternating pairs,512MiB/180s limits and unchanged10%/MAD gates.
The array before/after ratios are1.0852,0.9488,0.9304, all within their gates;
these do not establish an array speedup. Boa qualification applies to these
workloads and this observed environment, not arbitrary JavaScript programs.

The canonical sync adapter uses the actual modified Boa in omoikane at
007556adccb8fb4329236932ca8175f8e8d119d2, with original features and locks.
The adapter source, compiler and executable identities were checked; all worker
hashes are retained. Workers ran serially on guestCPU0 with agent threads on
CPUs1-3 and no concurrent native build, test or profiler. Affinity was restored
after both terminal reports. This does not control host physical-core placement.

Native661 passed/21 ignored in87 suites; Clippy, fmt and selected pinned Test262
array gates pass. Both public GC/descriptor integration tests pass strict-
provenance/symbolic-alignment Miri with isolation enabled. Hosted CI40792515990
passes all4 jobs, including stable, Rust1.85, Clippy/fmt and C ABI. The allocation
probe again requests65024 bytes in7 requests versus585216 bytes/7 for reference,
validates8386560 and retains0 cells; this is not timing, RSS or peak memory.

The offline auditor rejects7 corruptions of a complete real report. Run it with
`node scripts/audit-original-workload-records.mjs REPORT.json [...]`.
An audit verifies outcomes without requiring or fabricating a favorable result.
Hosted identity checks are also reproducible with
`node scripts/audit-omoikane-hosted-identities.mjs COHORT_DIR MEASURED_SHA BASELINE_SHA`.
The complete old Cow cohort passes; four metadata corruptions (added JIT,
wrong adapter hash, inconsistent candidate hash and wrong control revision)
are rejected. This identity audit is separate from the statistical audit.
The full library Miri auditor is reproducible with
`node scripts/audit-full-library-miri.mjs FULL_LOG STATUS_JSON EXPECTED_SOURCE OUTPUT_JSON`.
It requires119 distinct exact successes, no ignored tests, the complete16-shard
partition and all19 terminal jobs successful for that source. Against the
completed prior-source run it rejects wrong-source, missing-job and ignored-test
mutations. Those rejection checks do not qualify the still-running view source.

## Investigation of owned-descriptor overhead

`codegen-arm64` retains disassembly, symbol sizes, binary hashes and compiler
for all three frozen workers. In `own_element_value`, Cow has420 bytes of code
and208 bytes of entry stack allocation; the view has228/80. The view also calls
a separate252-byte/80-byte-stack `ElementRef::value` helper, so the individual
function shrink must not be presented as a reduction in the complete call
path. `try_get_own_property` uses304 entry-stack bytes for Cow and240 for view
and reference. These are static observations, not runtime cost measurements.

`callgrind-array` retains one whole-process diagnostic per frozen binary with
the original500000-iteration array body, one checked warmup and one checked
pass. All three validate124999750000 twice. Total instruction references are
8419414936 reference,8482373294 Cow,8397370199 view: Cow/reference1.00748,
view/reference0.99738 and view/Cow0.98998. Startup/setup are included. This modest
instruction difference is not a native-time or CPU-cycle result and does not
establish why the old hosted x64 cohort had broad regressions. Native array
before/after remains within its fixed gates. No old unfavorable report is
replaced by this diagnostic.

Hosted two-machine comparisons (run40792533180) and full latest-runtime security
(run40792515995) remain pending. No merge acceptance is implied. The initial
Cow trial and pre-GC regressions remain preserved separately and are not erased
by this ARM64 result. ArraySetLength double coercion #450 is outside this change.

`reference-drift-497` records an upstream check on2026-10-29. Omoikane main at
49715adf280c2787d7e3209bdfe76c64a2ead3d7 has exactly the same Git objects for
the complete vendored engine tree, Cargo.toml, Cargo.lock and build.rs as the
comparison pin. Thus the vendored Boa/regress and dependency configuration did
not change. Application JS/DOM code did change; this is not a timing or rebuild
of that newer application embedding. The comparison remains explicitly pinned.
