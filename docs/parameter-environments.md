# Parameter environments

Parameter lists containing initializers or computed binding keys prepare their
body var binding plan once with FunctionCode. Calls initialize parameters first,
then create a child environment for body vars. Redeclared parameter values are
copied into independent cells; defaults retain the original parameter cells.
Body-only vars remain invisible to closures created in defaults.

Ordinary calls, derived constructors and supported generators use this plan.
Async calls reuse ordinary call evaluation. Parameter lists without expressions
retain their shared environment path, including rest and plain destructuring.
The arguments binding used by defaults is kept outside body var redeclarations;
complete mapped/unmapped arguments semantics remain tracked by #166.

Public Ast/Auto cases live in `tests/parameter_environments.rs`. The earlier
source `88dd92e` passes the full-corpus regression gate, whole-library security
and hosted toolchain checks. Source `3c437b9` also preserves every preceding
pass in the complete corpus. Current rest-entry source `c4781cc` needs its own
complete qualification; native Boa speed qualification remains pending. The earlier eight
integration tests pass strict Miri, as do the two new supplied-default tests on
the optimized source. The implementation does not establish complete eval, generator or
async semantics.

Reference: [ECMA-262 2026 FunctionDeclarationInstantiation](https://tc39.es/ecma262/2026/multipage/ordinary-and-exotic-objects-behaviours.html#sec-functiondeclarationinstantiation).

Local proof is retained in `benchmarks/parameter-environments/correctness-v1`.
Replay with `node benchmarks/parameter-environments/correctness-v1/verify.mjs`.
It covers the initial 962-test all-target run, all six final integration tests
under native Ast/Auto, strict Miri and Rust 1.85.0, plus Clippy and i686.
The record does not qualify native Boa speed or full Test262 compatibility.

Initial fixed-workload observations are retained in
`benchmarks/parameter-environments/oracles-initial-v1`. Temiya validates all
eight inputs; pinned actual Boa validates three and throws in five (four
uninitialized-binding errors and one invariant failure). No input was dropped
or changed. These failures prevent this reference capture from qualifying
native speed; replay integrity success is not a performance pass.

Preparation preserves var declaration order while using hash membership for
parameter names, function names and duplicate vars. This avoids repeated scans
as parameter/var lists grow. A 192-parameter Unicode-name regression checks
initial values and repeated var declarations. No native speed claim follows
from this preparation change; earlier retained captures keep their source IDs.

The separate `large-preparation-v1` diagnostic retains one whole-process
Callgrind capture per source for the same 192-parameter/384-var input. Both
return 193; instructions decrease from 17,464,899 to 16,834,912 (3.607%).
This includes setup and is not a native Boa qualification. Replay with
`node benchmarks/parameter-environments/large-preparation-v1/verify.mjs`.

Integration with the assignment VM keeps parameter-only names outside
the body var descriptor. Verified body reads resolve those names through the
parent parameter record; redeclared body vars retain independently copied
cells. An internal VM entry accepts this prepared Declarative body record
while the public function-frame entry still requires a Function record.
Pure prepared bodies resolve only proven parameter names through this chain.
The context regression covers assignment defaults using this/new.target and
body function declarations shadowing a parameter without changing its closure.

The earlier integrated source `6263525` passes 1,022 all-target/all-feature tests
(26 ignored) and Clippy locally. All eight parameter tests pass strict Miri; earlier-source captures
above remain historical evidence.

`reference-diagnostics-v1` isolates the fixed reference engine using a bare
`boa_engine::Context` from the same clean, pinned vendored source. All eight
original inputs remain unchanged: three validate and five fail without browser
embedding. Four minimal programs isolate a parameter initializer plus a body
var redeclaration; the three redeclaration cases fail through both reference
entry points, while the control without a body var succeeds. All four validate
in the recorded Temiya6263525 build. Full inputs, exits, diagnostics, adapter source and
lockfile are retained with hashes. This diagnoses reference correctness and
does not qualify speed or claim the same failure in other Boa versions.

`integrated-correctness-v1` retains the integrated source identity, all
1,022 native test results, eight exact strict Miri results, Clippy and the
first validation of all eight unchanged10,000-iteration inputs. Its verifier
checks every compiler-input hash and each input/result pair. This source still
requires its own complete corpus, security and valid native Boa comparison.

After integrating the qualified assignment runtime and merged main, source
`88dd92e` passes 1,032 all-target/all-feature tests (26 ignored), fmt, Clippy
and all eight parameter integration tests under strict Miri. All eight unchanged
10,000-iteration inputs validate twice through its frozen worker. The later merge
of main at `1dfa282` changes history only and preserves all 84 compiler inputs.
`current-base-correctness-v1/verify.mjs` replays all logs, the eight inputs and
all 84 compiler-input hashes. These local results alone do not qualify whole-corpus,
whole-library safety or native Boa speed.

`corrected-reference-arm-initial-v1` retains the first native comparison against
the explicitly corrected pinned reference on the local ARM host. All outputs
validate, but 20 of 24 pairs are slower beyond the unchanged threshold and four
are within noise. This is adverse evidence, not speed qualification. Its
whole-process var-copy Callgrind diagnostic records 246,701,156 instructions;
environment creation, parameter binding and binding reads merit investigation.
Nayuta v0.3.3 was rechecked as the latest release and scanned the three relevant
Rust files, reporting 41 syntactic loop candidates without parser errors. These
candidates include tests and do not measure CPU time. No reproducible Nayuta
defect was found. Hosted two-machine results remain separate.

The optimized call path retains the original AST and metadata, and prepares a
verified ordinary template only for normal functions with identifier/default
parameters and no arguments/eval observers. At call time every defaulted
parameter must receive a non-undefined argument; otherwise the complete original
parameter/body-environment path executes. No initializer or parameter-created
closure executes on an accepted call. Missing plain parameters still receive
undefined. A final identifier rest binding uses a fresh intrinsic Array before
ordinary template entry, including rest-only functions. Patterns, async,
generators and methods retain their prior path.
Tests cover effects, falsy supplied values, missing/undefined defaults, escaping
closures, arguments, Function.length and constructor new.target.

`supplied-defaults-arm-v1/verify.mjs` retains all eight inputs and their outcomes,
1,036 successful native tests (26 ignored), Clippy, two exact strict Miri results,
all 85 compiler-input hashes and all integration-test source hashes. Var-copy and
constructor each beat the explicitly corrected pinned Boa in all three ARM
pairs; the other six do not qualify. Relative to the preceding fused-cell source,
six pairs are faster beyond threshold and 18 remain within noise. A completed
same-binary control is retained as variability evidence. The fused-cell source
alone reduced var-copy whole-process instructions from 246,701,156 to 232,268,442,
while all its native before/after pairs remained within noise. These local results
do not qualify all eight inputs, the inherited 70 cases, whole-corpus regression
preservation or whole-library security for the optimized source.

`full-supplied-defaults-v1` independently replays all 100,211 outcomes on source
`1c4e26f` (identical compiler inputs to `3c437b9`). All 38,801 preceding passes
remain passes with zero regressions or new passes. Its completed security run is
retained in `security-supplied-defaults-v1`: all 179 enumerated strict Miri tests
pass, while ASan aborts after 991 successful tests on a finite harness-ordering
test's 200 ms timeout. The security capture remains unqualified. The finite test
now uses the same 1,500 ms budget as existing instrumented completion tests,
retaining its exact completion/HarnessError assertions. The separate 200 ms
incomplete/nonreturning deadline tests are unchanged. `asan-lexical-read-v1`
replays the subsequent complete 1,039-test ASan-only success on `e4362bb`.

`hosted-pre-optimization-v1` retains all 84 comparison reports from both
unfiltered hosted jobs on source `be9edc6`. Both hosts fail the eight new
parameter workload gates; copy 1 also fails inherited object-binding-parameters.
These adverse observations are integrity-checked and cannot qualify later code.

For zero-parameter, synchronous expression arrows returning an identifier,
source `c12c6f7` resolves the captured environment afresh on every call. It adds
no activation bindings or this value. Live updates, object-environment getters,
outer arguments, TDZ/missing-name errors and captured objects across GC retain
their ordinary lookup behavior. Tests prove repeated primitive reads allocate
no empty managed activation records. `lexical-read-arm-v1` retains 1,040 native
passes (26 ignored), Clippy, three public strict Miri tests and the strict Miri
allocation test. The same default-closure whole-process Callgrind input decreases
from 538,234,647 to 494,523,211 instructions in one capture per source. Native
before/after has one faster pair and 23 noise outcomes; identical-binary control
has 24 noise outcomes. Var-copy qualifies two of three local Boa pairs and
constructor three; the other six still do not qualify. Every unfavorable pair is
preserved. Complete current-library Miri, current corpus and both-host native
qualification remain required before merging.

The same runtime at recorded commit `3ac5a5f` completes all 100,211 Test262
variants: 38,801 pass, 52,823 fail, 18 skip and 8,569 timeout, with no missing
results. All 38,759 preceding passes remain passes and 42 additional variants
pass. `full-current-main-v1/verify.mjs` independently replays both complete
captures and compares every outcome. This establishes regression preservation,
not complete language conformance. `security-current-main-v1/verify.mjs` checks
all 19 hosted jobs, all 177 exact strict Miri library tests against the complete
Miri enumeration, and 1,031 successful ASan tests. These records do not qualify
native Boa speed.

The separately retained `upstream-boa-diagnostics-v1` executes the same eight
inputs and four minimal probes on clean upstream HEAD329e897 (workspace version
0.22.0). The same five original inputs and three var-copy probes fail; three
original inputs and the no-body-var control validate. This debug-build correctness
diagnostic preserves every input and error and does not replace the pinned
benchmark reference or qualify performance.

The local `reference-patch-diagnostics-v1` experiment keeps the same eight
inputs and four minimal probes. Correcting the source parameter record lookup
and removing an unconditional undefined overwrite validates six originals and
all four probes; two pattern-expression cases still fail. A second local patch
also detects expressions inside binding patterns while keeping Function.length
independent of nested expressions. It validates all eight originals, all four
probes and eight additional controls for lengths, shadowing, nested arrays and
TDZ. Both attempts, including the two remaining first-attempt failures, are
retained. The verifier applies each patch to hashed upstream base files and
checks all input/result pairs. These are debug-build correctness diagnostics,
not stock-Boa results or native speed qualification; the benchmark reference
and the original input manifest remain unchanged.

`rest-entry-arm-v1` retains source `c4781cc`: accepted normal rest calls create
fresh real Arrays with the receiving Realm's intrinsic prototype, preserve
extra/missing arguments and original Function.length, and use ordinary verified
body cells. Undefined/missing defaults, arguments/eval observers and rest binding
patterns keep their prior path. Constructor context, array identity, prototype
setters, poisoned iterators, evaluation order and fallback default closures are
tested. Native tests pass 1,043 cases (26 ignored); Clippy, both new public strict
Miri tests and a forced-allocation-GC rest-array test pass. All 85 compiler inputs
and integration-test source hashes are retained. Rest-copy is faster than the
preceding implementation in two pairs (ratios about 0.817); the third pair is
within noise. It still loses all three corrected pinned Boa pairs. The remaining
22 before/after pairs and all 24 identical-binary pairs are within noise. A
whole-process rest-copy Callgrind capture per source decreases from 374,319,709
to 316,014,322 instructions. These results do not qualify the remaining six new
inputs or the complete current-source 78-input comparison/corpus/security gates.

Verified throw guards can now use the activation-free leaf executor when the
complete body is one forward `if (...) throw ...` followed by `return`.
All three expression fragments must qualify before any effect executes. Local
writes, captures, `arguments`, `eval`, additional branches and unsupported calls
retain the ordinary execution path. Literal string and unsigned integer property
keys can use named-property reads within a fragment; dynamic coercions fall back.
Getter/Proxy order, selected-tail exceptions and GC roots are covered by public
Ast/Auto tests and four strict Miri tests. Native tests pass 1,048 cases (26 ignored).

Source `e1e9000` diagnostics are retained in
`benchmarks/parameter-environments/guarded-leaf-arm-v1`; replay its `verify.mjs`.
The fixed rest input's whole-process Callgrind count fell from 316,014,322 to
273,475,107 instructions. Local native comparisons do not establish a speed
qualification: the rest before/after comparison has one faster pair and two noisy
pairs, and the same-binary control also has one faster pair. Rest remains slower
than corrected pinned Boa in two pairs and within noise in the third. All eight
original inputs and unfavorable measurements remain in the record. Current-source
full-corpus, complete security and two-host native qualification remain pending.

Historical rest-entry runtime `c4781cc`, dispatched at `9b6d8ee`, now has
complete retained hosted results. `full-rest-entry-v1/verify.mjs` extracts and
independently replays all eight full-corpus gates and compares every variant to
the preceding capture: 100,211 outcomes, 38,801 pass, 52,823 fail, 18 skip and
8,569 timeout, with all 38,801 preceding passes preserved and no new passes or
regressions. `security-rest-entry-v1/verify.mjs` checks all 19 security jobs,
including the complete 181-test exact Miri partition and 1,042 ASan passes.
These records qualify only that historical runtime's recorded checks; they do
not establish speed or qualify the later guarded-leaf or bulk-rest changes.

Source `77e343a` initializes a fresh real rest Array's prototype, length and
packed elements in the unpublished record before its managed allocation.
Initial object edges are checked against the receiving heap and traced at the
allocation safepoint. Numeric-only inputs use packed numeric storage; mixed
inputs retain complete ordinary writable/enumerable/configurable descriptors.
Both guarded template entry and ordinary parameter binding use this construction;
Array identity, intrinsic prototypes and the full fallback paths remain intact.

`bulk-rest-arm-v1/verify.mjs` retains 1,051 native passes (26 ignored), Clippy,
release, five public strict Miri tests, forced-allocation-GC and cross-heap Miri
units, all 86 compiler-input hashes and all integration-test hashes. The fixed
rest whole-process instruction count drops from 273,475,107 to 253,985,731.
Native before/after has 24 noisy pairs; the same-binary control has 23 noisy
pairs and one slower pair. Neither establishes a native improvement claim.
Against explicitly corrected pinned Boa, var-copy and constructor pass all three
local pairs; rest loses all three at ratios 1.186, 1.212 and 1.212. The six other
workloads remain unqualified, as does the complete two-host native requirement.

Source `b01f8d1` extends prepared-body execution to real-cell stores and complete
zero-argument calls. New body-var bindings are accepted only when the separate
parameter/body instantiation plan has already created them; otherwise only
existing parameter cells qualify. Calls use the ordinary undefined receiver and
shared Object call path, preserving Proxy apply, errors and GC roots. A complete
four-instruction call is proved before effects, retains its original PC layout,
and rejects any branch entering its interior. Unsupported operations anywhere
still select the full VM before execution.

`body-calls-arm-v1/verify.mjs` retains 1,054 native passes (26 ignored), three
public strict Miri passes, Clippy/release and complete source/test hashes. It
covers independent copied cells, callback mutation, default closures, receiver
and Proxy behavior, exceptions, fallback order and GC during calls. The fixed
computed-key input drops from 335,808,224 to 318,888,177 whole-process instructions.
Native before/after has 22 noisy pairs, one faster pair and one slower pair;
same-binary has 24 noisy pairs. Against corrected pinned Boa, var-copy qualifies
locally in two pairs and constructor in three, with all other six still failing.
All adverse/noisy results remain recorded. No complete native speed qualification
or current-source full-corpus/security qualification is claimed.

Source `a5d583e` permits an omitted default arrow to enter the ordinary template
when it has no parameters and its complete body is one identifier absent from
the entire enclosing parameter list. The complete header guard runs before any
initialization. Such an arrow is freshly constructed by ordinary named evaluation
in the enclosing environment, preserving inferred name, source, live lookup and
receiving Realm. Reads of any parameter (including later/rest names), arguments,
eval, this/new.target, async arrows and other missing initializer expressions
retain full instantiation. Provided non-undefined defaults still skip evaluation.

`outer-default-arm-v1/verify.mjs` retains 1,057 native passes (26 ignored), two
public strict Miri passes and two forced-allocation-GC Miri units, Clippy/release,
all source/test hashes, all unchanged native inputs and control measurements.
Public checks cover fresh identity/metadata, named functions, live outer/with
bindings, TDZ timing, parameter capture, lexical contexts and mixed initializer
ordering. The fixed default-closure whole-process instruction count decreases
from 466,579,851 to 457,084,275. All three default-closure before/after pairs are
within noise, and it still loses all three pairs against corrected pinned Boa
(ratios 1.463, 1.482, 1.469). Across the full suite, before/after has 23 noisy
pairs and one faster pair; same-binary has 24 noisy pairs. Native speed and
current-source full-corpus/security qualification remain incomplete.
