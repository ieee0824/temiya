# Bytecode closures (work in progress, #292)

Function declarations, expressions and arrows now create Object callables whose bodies execute
only verified bytecode. `FunctionTemplate` stores immutable name/parameter/strictness
metadata and independently verified nested code; it contains no AST or runtime
Environment. `CreateFunction` captures the active actual Environment, and instances
share the template through Rc rather than cloning executable bodies per closure.
The verifier requires scoped creation, a Function scope at body entry, matching
strictness, and valid parameter uniqueness. Function metadata cannot be loaded as
a language value.

Calls instantiate actual Function Environment records and simple identifier
parameters. Ordinary functions bind dynamic this and an arguments object; arrows
resolve this, arguments and newTarget through their captured outer records. Strict
functions preserve primitive this; sloppy functions use the host-supplied global
this or real primitive wrapper slots/prototypes. Constructors retain newTarget,
use its prototype (falling back to Object.prototype), and honor object-valued
returns. Function objects use Function.prototype and the existing lazy prototype
implementation; arrows have no Construct method or own prototype. Named expression
self bindings use an immutable name environment. Native callbacks hold only a
weak self slot, preserving the existing ownership policy.

The integration tests compile complete test programs through the public VM API;
no AST bootstrap supplies their functions and no AST evaluator executes a body.
They cover escaped shared mutations after code/Realm handles are dropped, lexical
this/arguments/newTarget, host global this, simple parameter/var reuse, named
recursion, per-iteration **while-block** captures, TDZ/const/throw propagation,
overflow recovery and wrapper prototypes. Existing Rust-stack call depth 32 and
argument count 65,536 limits remain in effect. Keeping captured records alive is
not a claim that reference cycles are collected; tracing/GC remains unfinished.

#292 remains open. Sloppy block/Annex B declarations, for-let per-iteration
records, default/rest/destructuring parameters, mapped sloppy arguments and the
full arguments descriptor/prototype contract, async/generators, inferred names,
original pinned Test262 results, latest CI/security and performance validation
remain incomplete. Unsupported source forms fail compilation explicitly.
`compile_function_body` retains its existing caller-prepared no-parameter API.
Engine continues to run the AST backend (#307). Broader builtin limitations,
including missing BigInt.prototype.valueOf, are separate from correct wrapper
slot/prototype creation. No whole-function conformance or speed gain is claimed.

## Hoisted declarations

Scope metadata now records selected function templates and the binding scope's
entry PC separately from the capture scope. Global declarations first validate
all lexical restrictions, var/lexical conflicts, function eligibility in reverse
selected order, and remaining var eligibility before creating any bindings or
function objects. Initialization then creates lexicals, selected functions in
source order (last duplicate wins), and remaining vars. Tests compare own-key
order with the AST backend and check accessor replacement without invoking it;
failed function/var eligibility leaves newly declared lexicals/vars absent.

For simple-parameter strict functions, top-level lexical bindings live in the
Function record, as FunctionDeclarationInstantiation step 31 requires. Non-strict
function body declarations capture a separate body lexical record while their
mutable bindings belong to the Function var record. The caller-prepared legacy
body API preserves its existing separate body-lexical-record contract. Both paths
create TDZ bindings before function objects. Declaration templates have names
without expression-private self bindings, so a saved declaration observes later
reassignment of its declared binding. Strict block functions initialize their
lexical bindings on block entry; sloppy block/if function forms remain explicit
compile errors pending the Annex B implementation. Parameters or direct body
lexical/function declarations named arguments suppress the implicit arguments
object, following the simple-parameter instantiation rules.

These changes follow [FunctionDeclarationInstantiation](https://tc39.es/ecma262/2026/multipage/ordinary-and-exotic-objects-behaviours.html#sec-functiondeclarationinstantiation)
and [GlobalDeclarationInstantiation](https://tc39.es/ecma262/2026/multipage/ecmascript-language-scripts-and-modules.html#sec-globaldeclarationinstantiation).
They do not complete non-simple parameter/arguments semantics or the original
Test262 gate.

## Fixed original Test262 audit

`pinned_test262_closure_audit` fixes twelve original files (17 variants) covering
name capture, function-body lexical scopes, defaults/rest, arrow this/newTarget,
and expression/declaration forms. It reads the pinned Git blobs unchanged,
compiles each entire test program through the chosen backend, and uses fresh
Realms. Original sta/assert/includes execute as AST-backed host harness for both
backends; test-program compilation failures never fall back to AST. Fixture blob
identities, both toolchain reports and all failures are retained in
[closure-audit.json](../test262/closure-audit.json).

On stable and Rust 1.85.0, before/after VM coverage changes from **2 pass, 1 fail,
14 compile-unsupported** to **4 pass, 2 fail, 11 compile-unsupported**, with zero
skips and no previously passing variant regressed. The two newly passing variants
execute the original returned-arrow lexical-newTarget fixture after constructor
completion. The newly executed failure is a declaration fixture exercising direct
eval; both remaining failures report an assertion error-constructor mismatch.
These failures and the unsupported non-simple parameters, updates and object
literals remain visible. AST coverage stays 10 pass / 7 fail, separately reported.

Both audits intentionally exit 101 because this selected gate remains incomplete;
these are not passing CI/conformance checks. Normal stable/MSRV all-target tests
pass 403 tests with 10 ignored, including this opt-in audit. #292 remains open.

```sh
TEST262_DIR=/path/to/test262 TEST262_REPORT=closure-audit.json \
  cargo test --test bytecode_closures pinned_test262_closure_audit -- --ignored --nocapture
```

The same original property selection also advances with the stacked call/function
work: stable/MSRV each report **9 VM pass, 0 fail, 10 compile-unsupported** across
19 variants, compared with 0 pass / 19 unsupported in the property-only revision.
AST remains 17 pass / 2 fail. Full results are in
[property-reference-followup-audit.json](../test262/property-reference-followup-audit.json).
This is a follow-up result for this stack, not evidence that #469 alone passes
those tests. The property audit still exits 101 and #288 remains incomplete.
