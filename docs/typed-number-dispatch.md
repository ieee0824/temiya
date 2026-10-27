# Typed Number dispatch and checked binding reads

The arithmetic diagnostic identified substantial instruction-reference counts
in VM binary dispatch, string operator matching and operand management. The
existing integer remainder fast path is retained. The diagnostic used a reduced
20,000-iteration input under Callgrind, so its instruction counts and instrumented
elapsed times cannot establish a speedup on the original two-million input.

The implementation shares one primitive-Number operation table between AST and VM.
The VM retains its existing typed `BinaryOperator` for Number pairs and reuses
their operand slots, preserving roots beneath them. Primitive arithmetic has
no JavaScript callbacks. Other operand kinds and operators outside that table
continue through the existing Realm-aware conversion and error paths. Operations
are evaluated separately, without floating-point reassociation or contraction.
No new syntax, fallback retry, skipped binding validation or benchmark-specific constant
is introduced.

Binding reads also copy an actual stored Number directly instead of dispatching
the full Value clone. Record borrowing, name matching, initialization/TDZ and
write/const checks remain in place. Other kinds keep their owned clones; there
is no cached type assumption or skipped lookup. Bit-preservation and captured
update regressions cover this path in both AST and explicit Bytecode.

Validation includes the existing 2,653-case independent JavaScript oracle in
AST/VM, the IEEE-bit/BigInt bitwise oracle, and public Engine regressions for
array/object/pending-call roots, signed-zero/NaN/Infinity/comparison results,
ordered object conversion, string concatenation and abrupt throw identity.
Five unmodified fixed Test262 files cover fractional remainder, infinite divisors,
signed zero, multiplication overflow and BigInt fallback. All 20 executions
(strict/sloppy and AST/Auto) passed locally; Auto is asserted to use actual
Bytecode for each test body. Corpus-dependent tests are explicitly run in CI:

```sh
TEST262_DIR=/tmp/temiya-test262 cargo test --locked --test typed_number_dispatch \
    pinned_test262_typed_number_dispatch --all-features -- --ignored --nocapture
```

The optional `include_controls` input of `omoikane-benchmark.yml` adds a direct
baseline/actual-Boa comparison and a same-executable candidate control after the
ordinary before/after and candidate/Boa reports. It requires an Auto Script
baseline and probes both modes before measurement. Same-binary faster/slower
labels measure variability, not code improvements. `two_runners` records two
independent hosted machines with separate artifacts; it does not select or
pretend to guarantee an Intel CPU. Default inputs retain the original two reports
and artifact name. Every comparison keeps the original eleven workloads,
15 samples plus checked warmup, three alternating pairs, oracle and watchdogs.

An optional `required_cpu_model` substring requests a specific target, such as
Intel Xeon 6973P-C. Each job records its actual models before building. If the
target is unavailable, it retains an availability record and explicitly reports
that no measurements ran; that job status is never a correctness or speed pass.
The default empty requirement accepts any recorded CPU. This selects declared
hardware, not a favorable performance outcome.

## Recorded candidates and limits

The latest runtime measured for this change is
`37e0996850829fdaa71bae34e99e8d04edd189b9`; baseline main is
`fdd4ac6d1d5b4bd3b3287d0943557c9ea1d6496c`. The earlier stages are typed dispatch
`b8b0f6b07e1302fc2a980ef9cddaf9f2d2102274` and direct Number copies
`4fb82dbc90e23a2dc14ab906665d8e7209a62887`. Inline annotations on VM binary
operations, checked function binding reads and exact JsString equality allow the
compiler to optimize call boundaries; they do not remove any runtime checks.

The full local release all-feature validation passed 512 tests, with 18 ignored
tests explicitly excluded from passes. Fixed Test262 separately passed 20 actual
executions, and Clippy passed. Source CI 40560067443 passed formatting/Clippy,
stable, MSRV 1.85.0 and C ABI checks; manual security 40560102648 passed all three
checks. These are targeted validation results, not full ECMAScript conformance.

Every original-eleven report uses the unmodified pinned omoikane fixture and
iterations, 15 samples plus a checked warmup, three alternating pairs, 512 MiB and
180 seconds. A complete report has 66 worker invocations, 1,056 checked results
and 33 pair decisions. Actual pinned application JsRuntime is used rather than
upstream Boa. Invocation parse/evaluation remains timed. Temiya drops its per-invocation
Script handle before stopping the timer; actual Boa uses the complete pinned
public eval call. Setup/startup/warmup are separate. Noise is never a win.

| Runtime / machine | Before-after faster / noise / slower | Candidate / actual Boa | Same executable control |
| --- | --- | --- | --- |
| b8b0f6b0 / ARM64 | 7 / 25 / 1 | 33 / 0 / 0 | 0 / 33 / 0 |
| b8b0f6b0 / AMD EPYC 9V45 | 15 / 18 / 0 | 33 / 0 / 0 | 0 / 33 / 0 |
| b8b0f6b0 / AMD EPYC 9V74 | 13 / 20 / 0 | 33 / 0 / 0 | 1 / 32 / 0 |
| 4fb82dbc / ARM64 | 13 / 20 / 0 | 33 / 0 / 0 | 0 / 33 / 0 |
| 4fb82dbc / Intel Xeon 6973P-C | 12 / 21 / 0 | 31 / 2 / 0 | 2 / 31 / 0 |
| 37e09968 / ARM64 | 20 / 13 / 0 | 33 / 0 / 0 | 0 / 30 / 3 |
| 37e09968 / AMD EPYC 9V45 (copy 1) | 27 / 6 / 0 | 33 / 0 / 0 | 0 / 33 / 0 |
| 37e09968 / AMD EPYC 9V74 (copy 2) | 17 / 16 / 0 | 33 / 0 / 0 | 0 / 33 / 0 |

Hosted x64 run 40562000380 copy 1 (artifact 12762482172) is complete. Direct
baseline/Boa has 32 faster and one noise decision; arithmetic pair 0 is noise
(ratio 1.007663). Copy 2 (artifact 12763052819) also completed; its direct baseline/Boa comparison
has 33 faster decisions. Both hosted same-executable controls have 33 noise
decisions. Source 37e09968 beats actual Boa in all 99 original-eleven comparisons
across these two x64 hosts and the recorded ARM64 host. This does not satisfy
the separate Intel target or establish arbitrary-machine/application superiority. The ARM CPU model is unavailable; the report honestly records
ARM64, Linux, four logical CPUs and the observed compiler. The local synchronous
baseline binary retains source def9649b; git diff verified its runtime, worker
and Cargo inputs equal baseline main fdd4ac6d. It is not relabelled as main.

On ARM64 the latest arithmetic before/after ratios are
0.736628 / 0.744730 / 0.767190, and candidate/actual-Boa ratios are
0.574141 / 0.575439 / 0.572948. They apply only to that recorded machine and input.
The initial b8b0f6b0 ARM prop-mono pair 1 was slower (ratio 1.335053, threshold
0.1); it is retained and its cause is not asserted. Latest ARM same-executable
controls have slower labels for prop-mono pair 2 (1.425354), object-alloc pair 0
(1.329411) and array pair 2 (1.171507), all at threshold 0.1. Those controls use
identical executable hashes and show variability, not an implementation change.
All direct baseline/Boa ARM comparisons also have 33 faster decisions.

### Intel arithmetic remains unfinished

Run 40555924150 copy 1 actually matched Intel Xeon 6973P-C and measured source
4fb82dbc, baseline main and pinned actual Boa on the same host. Arithmetic
before/after ratios are 0.800354 / 0.801314 / 0.802217, all faster decisions.
Candidate/Boa ratios are 0.940782 / 0.911639 / 0.649241, with thresholds
0.133432 / 0.1 / 0.267489: two noise decisions and one faster decision. Issue
#510 requires all three to beat the fixed threshold and is therefore still open.
Direct baseline/Boa arithmetic is 1.136285 / 0.987712 / 0.788539 (slower, noise,
faster), while the same-executable arithmetic control is
0.879592 / 0.987928 / 0.997215 (faster, noise, noise). The full identical-binary
control has two faster labels and 31 noise labels, none a source improvement.
Do not infer an unmeasured baseline loss or discard environment variation.

The inline source's 48 declared-target probes across Ubuntu 24.04 and 22.04
obtained no Xeon 6973P-C. Availability records retain all actual CPU models and
skip measurements before building. Successful skipped jobs count as neither
correctness nor speed passes. General AMD/ARM qualification does not substitute
for the Intel requirement. No self-hosted runner was available at inspection.

### Additional actual async evidence

Three authored checkpoint inputs remain separate from the original eleven,
with n=2048 and independent expected values 2048 / 2096128 / 2048. Scheduling,
actual job drain and completion evaluation remain inside timing. Each actual-Boa
report passed fourteen positive/negative native adapter checks, eighteen worker
invocations and 288 checked results. The b8b0f6b0 report has 7 faster / 2 noise /
0 slower decisions; 4fb82dbc and 37e09968 each have 6 / 3 / 0. None meets an
all-nine-faster gate. A separate ARM Temiya-only before/after comparison has all
nine decisions within noise and is not an actual-Boa comparison. Cross-run and
cross-machine differences do not establish a before/after regression.

### Instruction diagnostics

Reduced 20,000-iteration Callgrind records are retained for baseline, typed
dispatch and Number-copy stages. Original two-million-iteration diagnostics
also independently verify original input hashes, the BigInt oracle 64, and both
warmup and sample results. Baseline runtime fdd4ac6d records 9,754,545,469
instruction references; 4fb82dbc records 7,902,546,346; 37e09968 records
7,322,535,307. Workflow-only diagnostic revisions and exact machines, build
flags, compilers and binary hashes are recorded. Whole-process instruction
references include startup/setup/warmup; they are not hardware cycles or valid
uninstrumented elapsed-time comparisons. Neither reduced inputs nor instrumented
times establish a performance acceptance result.

[Complete evidence and reproduction notes](../benchmarks/typed-number/README.md)
include every raw timing sample, numerical oracle, negative observation, native
adapter check, compressed raw profile, availability record and file hashes.
Phase and memory summaries retain every workload and both workers. Sampled
whole-process HWM is not retained heap, allocation counts or GC efficiency;
reported embedding startup excludes process launch, and setup is not a dedicated
parse-only suite. No complete application, Promise, GC or SpiderMonkey performance
superiority is established. Issue #427 remains open for its remaining scope.

The local actual-Boa synchronous binary was built from the preserved historical
adapter source in local-sync-worker-source.rs (SHA256 031ee68a...), whose rustfmt
output is byte-for-byte identical to the current adapter (efba2d69...). Its
original build note refers to the initial diagnostic; the exact frozen binary
was subsequently used for the complete repeated ARM reports. No scripted rebuild
is claimed for those cohorts. Actual pinned application, lock hashes, feature
sets, compiler, sandbox and numerical checks remain recorded.
