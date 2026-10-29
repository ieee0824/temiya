# Numeric/borrowed descriptor view, source6fb3160

Runtime source: `6fb3160be6d966b7b0c35fb7b5da06ddb33b3583`.
Reference main: `c7c4f265c4002de1d90223eccaa64aaafdada52a`.
The runtime replaces full owned read descriptors with a small numeric or
borrowed descriptor view. Ordinary flags and managed owner checks are retained.

| ARM64 CPU0 comparison | Faster beyond gate | Within gate | Slower beyond gate |
| --- | ---: | ---: | ---: |
| Candidate / reference Temiya | 0 | 33 | 0 |
| Candidate / actual modified Boa | 33 | 0 | 0 |

| Hosted AMD EPYC7763 cohort | Candidate/main | Candidate/actual Boa | Baseline/Boa | Same binary |
| --- | --- | --- | --- | --- |
| Copy1 |29 noise /4 slower|32 faster /1 noise|33 faster|33 noise|
| Copy2 |1 faster /27 noise /5 slower|33 faster|33 faster|33 noise|

Hosted run40792533180 completed collection successfully, but these results do
not meet the performance objective. Each of its8 reports independently validates
66 workers/1056 values, original inputs/gates and all source/build/control identities.
Both cohorts regress all3 primitive-string-property pairs by11.4–12.3%; each
also regresses string-concat pair2 by10.9%. Copy2 additionally regresses
prop-mega pair0 by17.3%. All6 hosted array decisions are within their gates.
Copy1's primitive-string-property/Boa pair1 ratio0.747106 fails its0.532667 MAD
gate. The other cohort's favorable result does not erase that miss. All raw
reports, controls, metadata and independent audits are in `hosted`.

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

Full runtime security(run40792515995) passes all19 jobs:119 distinct exact Miri
successes,0 ignored, the complete16-shard partition, ASan and dependency audit.
Full source/status/log hashes and the independent audit are retained. The special-
payload integration test was further strengthened at2ad5555 after timing and
again passed native/Miri2/0 ignored, without changing runtime/worker/input code.
Hosted regressions
and the Boa gate miss require investigation; this candidate is not accepted for
merge. The initial
Cow trial and pre-GC regressions remain preserved separately and are not erased
by this ARM64 result. ArraySetLength double coercion #450 is outside this change.

`reference-drift-497` records an upstream check on2026-10-29. Omoikane main at
49715adf280c2787d7e3209bdfe76c64a2ead3d7 has exactly the same Git objects for
the complete vendored engine tree, Cargo.toml, Cargo.lock and build.rs as the
comparison pin. Thus the vendored Boa/regress and dependency configuration did
not change. Application JS/DOM code did change; this is not a timing or rebuild
of that newer application embedding. The comparison remains explicitly pinned.

Raw Callgrind records use lossless gzip; `provenance.json` retains each
uncompressed SHA256 and the compressed file hash. Decompress before re-annotation.

The completed x64 diagnostic40797282661 is retained in `x64-profile`. All6
instrumented runs preserve original inputs/counts/oracles and validate warmup
and one pass. Both rebuilt executable hashes exactly match the native reports
on both hosted machines. Binaries remain in the workflow artifact; raw counters,
exclusive annotations, symbols and relevant disassembly are retained here.
For primitive-string-property, whole-process Ir rises0.896%; the largest exclusive
deltas are Binding::write+14000049 and set_function_binding_at+4000016.
The x64 write path adds a full32-byte incoming-Value copy and another initialized-
slot check around general store_owned. String-concat likewise adds about2.8M
exclusive Ir in Binding::write. This localizes unnecessary general binding-write
work; it is not proof that those counts explain the entire native slowdown.
Modeled cache/branch events are explicitly not hardware measurements. A guarded
number-to-number binding overwrite will be evaluated as a distinct runtime
change while preserving TDZ, mutability and all nonnumeric owner/barrier paths.
