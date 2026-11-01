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

Public Ast/Auto cases live in `tests/parameter_environments.rs`. Full corpus,
strict Miri, stable/MSRV/target checks and native Boa speed qualification remain
pending. The implementation does not establish complete eval, generator or
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

Integration with the current assignment VM keeps parameter-only names outside
the body var descriptor. Verified body reads resolve those names through the
parent parameter record; redeclared body vars retain independently copied
cells. An internal VM entry accepts this prepared Declarative body record
while the public function-frame entry still requires a Function record.
Pure prepared bodies resolve only proven parameter names through this chain.
The context regression covers assignment defaults using this/new.target and
body function declarations shadowing a parameter without changing its closure.

The integrated source passes 1,022 all-target/all-feature tests (26 ignored)
and Clippy locally. Its eight parameter tests are undergoing fresh strict
Miri validation; earlier-source captures above remain historical evidence.
