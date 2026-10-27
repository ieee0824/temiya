# Authored RegExp comparison

Three normal-case JavaScript inputs extend #427 without changing the original
eleven omoikane workloads. They use the public default Engine and actual pinned
omoikane JsRuntime, with the same invocation parse/evaluate boundary, separate
startup/setup/warmup, fifteen samples, three alternating pairs, 512 MiB and
180-second watchdogs. Every warmup and sample must return its independent oracle.

| Input | Count and oracle | Included work |
| --- | --- | --- |
| regexp-construction | 128*(8+5+2)=1920 | Varying pattern text, construction, exec and numbered capture lengths |
| regexp-global-exec | 2048*(3+4+2)=18432 | Reused global expression, complete scans, failed-exec reset and final non-match |
| regexp-unicode-captures | 1024*(4+2)=6144 | Reused Unicode expression, alternating astral/ASCII matches and UTF-16 capture lengths |

Construction is not a compile-only measurement. Pattern strings vary within a
pass but repeat across samples; engine caches are permitted and their execution
is not inferred. The suite does not cover lookarounds, backreferences, named
groups, JavaScript lastIndex property descriptors, match-index properties,
indices/Unicode-set flags, lone surrogates or full ECMAScript RegExp conformance.

The initial ARM64 comparison used frozen Temiya source
37e0996850829fdaa71bae34e99e8d04edd189b9, whose runtime/worker/Cargo inputs equal
merged main 9d67c8c0a200e42b62dec3bbcb7917f78d903add. Actual application revision
007556adccb8fb4329236932ca8175f8e8d119d2 and its original dependency lock/features,
public sandbox and modified Boa/Regress were retained. The historical local
adapter source/hash is preserved in typed-number/local-sync-worker-source.rs;
its formatting-only relation to the current adapter is separately recorded.
No new native worker build is claimed for this run. Both binaries and the actual
comparison/reference source bytes were independently checked before archiving.

All eighteen native workers and 288 values were correct. Construction is slower
in all three pairs: Temiya/Boa elapsed ratios 8.9365 / 8.6763 / 8.8280. Global exec
is faster at 0.6721 / 0.7040 / 0.6792, and Unicode captures at
0.8070 / 0.8017 / 0.8110, all beyond the unchanged 0.1 / three-MAD rule. The
all-three-input speed gate is false. These ARM results establish neither the
cause of construction cost nor performance on other machines. Do not select
only the six faster pairs or infer a source improvement from another cohort.

A separate one-sample native smoke run checks both real workers and repeated
state; it is not speed acceptance. Six malformed-kind/reference/coverage/
iteration/source/flag cases are rejected before a measurement report. A public
Engine regression runs all inputs twice in AST and Auto, asserting actual
Bytecode selection for Auto. No skipped or unexecuted tests count as passes.

    node scripts/extract-regexp-workloads.mjs NEW_INPUT_DIRECTORY
    TEMIYA_REVISION=MEASURED_TEMIYA_SHA node scripts/compare-omoikane-workloads.mjs \
      --regexp NEW_INPUT_DIRECTORY TEMIYA_WORKER ACTUAL_BOA_BUILD_INFO 15 3
    cargo test --locked --release --all-features --test regexp_benchmark_workloads

The flag must precede optional --temiya-baseline. Default comparisons keep the
original eleven and its exact provenance checks. An authored manifest cannot
enter the original suite or vice versa. Authored RegExp SpiderMonkey mode is
explicitly unsupported until its RegExp backend/mode assumptions are verified;
no native/JIT execution is fabricated.

[Raw initial observations and checked source snapshots](../benchmarks/regexp/)
retain the slower results. The construction gap needs profiling and a general
implementation improvement. Dedicated parse/startup/objects/GC groups and the
rest of #427 remain unfinished, and no application-wide or collector superiority
is established.

## Construction diagnostic

Run40574346710 (artifact12766766527) profiles the original authored128 iterations,
without reducing or replacing the input. Workflow-only revision654a68e3 has the
same runtime/worker/dependencies as this baseline. Both checked passes returned
1920 and actually used Bytecode. Callgrind records151,016,279 whole-process
instruction references; RegexBuilder::build accounts for124,230,676 inclusive
references (82.26%), with256 calls across warmup and sample. These figures include
startup/setup and are not native cycles, exclusive costs or elapsed-time speed
acceptance. They identify matcher construction as a significant instruction
source on the recorded x64 host; they do not establish its exact share of the
ARM elapsed-time gap.

Issue #513 tracks the general construction improvement. Pattern compilation,
automata/cache creation and allocation need separation before choosing a fix.
The other RegExp feature issues remain independent.

The [rejected Number operand experiment](../benchmarks/rejected-number-operand/)
is also preserved. Direct comparisons against current main did not establish an
arithmetic win beyond the fixed threshold, and two AMD property pairs were slower.
Its code stays inactive; favorable comparisons against an obsolete baseline and
other-host async results are not adoption evidence.
