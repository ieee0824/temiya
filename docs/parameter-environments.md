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

Public Ast/Auto cases live in `tests/parameter_environments.rs`. The recorded
source passes the full-corpus regression gate, whole-library security and hosted
toolchain checks; native Boa speed qualification remains pending. The eight local
integration tests pass strict Miri. The implementation does not establish complete eval, generator or
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
