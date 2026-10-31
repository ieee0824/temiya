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
