# Base class qualification evidence

`initial` retains source `3bc78ae`, including unfavorable measurements. The
unchanged original-eleven workloads have 33 faster decisions against actual
modified Boa on the recorded ARM host and 33 before/after decisions within
noise. The same-binary control has 31 noise, one faster and one slower decision.
This result alone does not establish class performance.

The separate authored class suite was frozen before execution. Construction,
method and static-loop cases produce nine slower decisions; the definition case
produces three within-noise decisions. Its same-binary control has 11 noise and
one faster decision. These results prevent claiming that this initial class
implementation is faster than Boa. Counts, bodies, oracles and eligibility must
remain fixed during subsequent optimization comparisons.

Both workers validate every warmup and all 15 samples in each of three
alternating process pairs. The same 180-second and 512-MiB external watchdogs
apply. Each timed pass parses and evaluates the invocation; startup, class/shared
setup and warmup are excluded. Class fixtures are separate from the original
eleven, not a replacement. No JIT or whole-engine superiority follows.

```sh
node benchmarks/base-classes/verify-initial.mjs
```

The verifier checks all compressed/decoded identities, reconstructs original and
class measurements and statistical decisions, verifies worker identity and
checks the retained 725-test run and the failed-then-fixed caller regression.
The source, raw data and conditions remain available even after later fixes.
Integrity success preserves slow results; it is not performance acceptance.

The intermediate `auto-functions-v1` capture uses clean source `d818832`.
Eligible Auto function bodies now use verified bytecode, while explicit Ast
continues using AST evaluation. The fixed class suite improves in 11 of 12
comparisons against the initial implementation, with one within noise. Against
actual Boa, six comparisons are slower and six remain within noise. Its
same-binary control has twelve within-noise decisions. This intermediate version
still does not meet the class speed goal. All three raw reports, worker identity,
controller snapshots, and the 728-test log are retained. Functional result JSON
records 256 passes and 38 dependency failures in each backend and a passing
214-variant PR gate; the JSON alone is not a reconstruction of those CLI reports.

`node benchmarks/base-classes/verify-auto-functions.mjs` independently checks the
class measurements and their statistical decisions and validates archived file
identities and the recorded functional result summaries. Original-eleven
measurements are separate and cannot establish class speed.

The same intermediate ELF was requalified on the unchanged original eleven:
33 faster decisions against actual Boa and 33 before/after decisions within
noise. The baseline/Boa control records 32 faster and one within noise; the
identical candidate ELF control records 32 within noise and one faster. All four
raw reports are retained and independently recomputed by the intermediate
verifier. Control labels do not establish a source-level improvement.

`known-ast-for-capture` preserves the authored CLI reproduction and both raw
backend reports for [#537](https://github.com/ieee0824/temiya/issues/537). Explicit
Ast loses per-iteration for-let header captures while Auto's verified body
preserves them at source d818832. This pre-existing loop gap remains open and is
not original Test262 evidence or a failure fixed by the primitive-slot change.

`lexical-v1` retains source `2f2e61d` after private lexical slots, empty block
elision and avoiding unused call-local roots. Its fixed class comparisons record
two faster and ten within-noise decisions against Boa, three faster and nine
within noise against d818, and twelve within noise for the same executable.
These observations do not establish the all-class speed goal. The correctness
archive preserves every raw class/PR/for-loop capture, all 391 selected original
for-loop inputs and harness/license files, plus the 731-test run. The complete
385-file/758-variant for selection has unchanged outcomes and failure signatures:
Ast 196 pass/562 fail, Auto 200 pass/558 fail, no skips/timeouts/missing variants.
These existing failures remain failures.

```sh
node benchmarks/base-classes/verify-lexical.mjs
```

The verifier reconstructs all class measurements, safely extracts the hashed
correctness archive, independently verifies its Test262 reports and per-variant
before/after comparison, and checks original input hashes and test counts.

Hosted measurements can include the same separate class suite with the
`include_classes` dispatch input to `omoikane-benchmark.yml`. The selected
`baseline_revision` must support these classes (for example d818832). Enabling
`include_controls` retains baseline/Boa and identical-binary controls for both
suites. `two_runners` records both machines without selecting a CPU or discarding
unfavorable results. CI success means valid observations, not a speed pass.

`hosted-f1c` retains both machines from run40960178364 and all eight reports per
machine. Both report three class-method comparisons slower than actual Boa. The
original eleven have 33 faster comparisons against Boa on each machine, but the
before/after comparisons retain one slower closure-allocation pair on each
machine. The identical-binary controls do not establish a source change. These
outcomes are not relabeled as passing class performance.

`full-f1c` retains all eight complete production shards from run40960620020:
52,133 files, 100,211 variants, 25,887 pass, 65,363 fail, 18 skip, 8,943 timeout,
zero missing/unexecuted variants. Its gate correctly rejects the known-pass
Sloppy variant of function/S13.2.2_A12.js. This exposed bytecode `this` lookup
incorrectly calling a Function-record operation on a declarative block; source
551d925 fixes the lookup and adds real Engine coverage. The original is now
included in the expanded 129-file/216-variant PR policy. Neither the old failure
nor any timeout/skip is a pass. This historical full snapshot is not a current
language conformance claim.

```sh
node benchmarks/base-classes/verify-hosted-f1c.mjs
node benchmarks/base-classes/verify-full-f1c.mjs
```

The full verifier safely decodes the retained capture and original reference,
independently checks every raw shard, proves complete nonoverlapping file/variant
coverage and reruns the production per-variant gate, including its rejection.

Latest native loop-binding measurements (runtime05d2eee) are retained in
`native-loop-v1`. The same fixed four class inputs produce six comparisons
faster than Boa beyond the noise threshold and six within it. Before/after
has four faster and eight noisy comparisons; the identical-worker control
has twelve noisy comparisons. These results do not establish superiority
for every class workload. The archive also retains actual native test logs
(740 pass, 26 ignored, 96 groups), the actual ASan job log
(738 pass, 26 ignored, 95 groups), build identities and affinity records.

`node benchmarks/base-classes/verify-native-loop.mjs` verifies the archive,
reconstructs every recorded class decision and recomputes both test counts.
Original-eleven measurements, hosted runs and full-corpus qualification
remain separate evidence.

`full-loop-v1` retains all eight shards from run41004744713, runtime05d2eee:
52,133 files, 100,211 variants, 25,895 pass, 65,355 fail, 18 skip, 8,943 timeout,
zero missing or unexecuted variants. Every known-pass gate was independently
replayed against the decoded fixed reference and agrees with its saved output.

Run `node benchmarks/base-classes/verify-full-loop.mjs` to safely decode the
archive, verify each raw capture, prove nonoverlapping complete coverage and
rerun all eight production gates. This preserves known successes; it does not
qualify full language conformance or count failures/timeouts/skips as passes.

The same native candidate's focused raw correctness records are retained in
`native-loop-v1/correctness.tar.gz`. Run
`node benchmarks/base-classes/verify-loop-originals.mjs` to recheck both
149-file class captures against their unchanged source hashes and 38 recorded
dependency-failure signatures, compare every outcome and raw failure signature
in both 385-file for-loop captures with the retained prior candidate, and
replay the 129-file/216-variant production PR gate. A recorded dependency failure
remains a failure; neither this fixed selection nor its verifier establishes
complete class or for-loop conformance.

`native-loop-v1/originals.tar.gz` retains all four original-eleven reports:
changed/Boa has 27 faster and six noisy comparisons, before/after has one
faster and 32 noisy, and baseline/Boa has 30 faster and three noisy. The
identical-worker control has 31 noisy comparisons and two apparent slowdowns.
Those control results are preserved and cannot establish a source regression
or improvement. Run `node benchmarks/base-classes/verify-native-originals.mjs`
to reconstruct all 264 runs and 4,224 checked values without dropping any
outcome. This archive does not establish an unconditional speed pass.

Final hosted measurements (run41004747984, runtime05d2eee, baseline9970cd5)
are retained in `hosted-loop-v1`, with all eight reports on each machine.
Both machines record all 33 original-eleven comparisons faster than actual
Boa and all 33 identical-worker comparisons within noise. Original before/after
has no significant slowdown (copy1:33 noisy; copy2:one faster,32 noisy).
For class4, copy1 records 11 faster comparisons and one noisy class-definition
comparison; copy2 records all12 faster. Each named class workload has repeated
significant speedups on both machines. Class before/after has six faster and
six noisy comparisons per machine; both class identical-worker controls have
all12 within noise. All inputs, counts, oracles and thresholds remain fixed.

Run `node benchmarks/base-classes/verify-hosted-loop.mjs` to safely decode
the archive, independently recompute all16 reports and retain the noisy result.
These observations qualify the recorded suites and machines; they do not
establish universal engine superiority or erase the noisier ARM controls.
