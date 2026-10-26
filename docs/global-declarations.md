# Script global declarations

Public `Engine::compile`/`evaluate`, `ScriptRecord::evaluate`, and the global
`StatementEvaluator` entry points instantiate supported Script declarations
through the Global Environment Record's dedicated operations. A global `var`
or direct function declaration creates/reuses an own property of the global
object. `let` and `const` use its declarative record and do not create properties.
For example, `var declared=1; this.declared` now evaluates to 1. A direct function
is available through both its identifier and the corresponding global property.

Instantiation checks lexical conflicts, then var/function conflicts, then
function and var eligibility before creating bindings. For ordinary global
objects, a failed declaration creates no other bindings from the same Script.
Function declarations choose the final declaration of each name; the surviving
functions are initialized in source order. Lexical bindings start in the TDZ,
and const remains immutable. A var declaration without an initializer preserves
an existing property's value and descriptor, including accessors, without
calling its getter or setter. Explicit initializers perform normal assignment
with the code's strictness. A rejected write to a non-writable global therefore
throws in strict code and preserves its value in sloppy code.

New global var/function properties are writable and enumerable, and are
non-configurable for Script declarations. Existing configurable properties may
be reconfigured for function declarations. An existing non-configurable function
target must be a writable, enumerable data property. Global object restrictions
are checked independently from lexical-record extensibility.

These rules follow [ECMAScript 2026 GlobalDeclarationInstantiation](https://tc39.es/ecma262/2026/multipage/ecmascript-language-scripts-and-modules.html#sec-globaldeclarationinstantiation)
and [CreateGlobalVarBinding](https://tc39.es/ecma262/2026/multipage/executable-code-and-execution-contexts.html#sec-createglobalvarbinding).
The 2026 lexical eligibility check uses existing lexical declarations and
restricted global properties, rather than a historical var-name list. Thus a
pre-existing configurable property reused by `var` can subsequently be shadowed
by `let`; its object value remains unchanged. This also holds when the global
object is non-extensible. The existing `has_var_declaration` API retains its
legacy bookkeeping behavior; it no longer determines lexical eligibility.

Function, block, and module environments keep their separate declaration
algorithms. Tests cover public Engine observation through `this`, same-Realm
successive Scripts, independent Realms, descriptor preservation, strict/sloppy
initializers, accessors, ordinary-global preflight, and AST/explicit-VM global
state/error equivalence. Module exports and function/block locals stay off the
global object. Engine backend selection remains work in #307.

The pinned Test262 runner loads original test and harness bytes using `git show`
at `d53ebc8b1931d9f1541e43d59650776e4b766248`. Its test-only `$262.evalScript`
callback invokes real `ScriptRecord::parse` and `evaluate` in the same Realm.
It supplies no other `$262` hooks and does not complete #147. Runtime-negative
cases must parse before producing the expected runtime error; parse-negative
cases must fail at parsing. Unsupported metadata and failed harness initialization
are never passes. Class execution, direct eval, Annex B rules and exhaustive
Proxy-global behavior remain separate work.

```sh
scripts/test262
TEST262_DIR=target/test262 cargo test --locked --all-features --test global_declarations pinned_test262_global_declarations -- --ignored --nocapture
```

The default focused selection excludes fixtures whose original propertyHelper.js
cannot initialize due to the separate native-function prototype limitation (#467).
Their failures are retained in the full directory audit; descriptor behavior is
also tested directly through the public runtime API. No harness source is
rewritten to make a test pass.

Engine still executes AST code. Correct global bindings and successful tests do
not establish a speedup against Boa. The original eleven omoikane workloads,
result oracles and repeated comparison/resource guards remain the performance
acceptance evidence.

Recorded local ARM64 results on stable and Rust 1.85.0: each full test run has
362 passes and 8 default ignored tests. The corpus-dependent focused test is
executed explicitly in addition to those counts:
[stable](../test262/global-declaration-focused-stable.json) and
[MSRV](../test262/global-declaration-focused-msrv.json) each pass 14 variants
from seven original files, with no failures or skips.

The [full global-code audit](../test262/global-declaration-audit.json) retains
all 42 files / 75 variants against the #465 baseline and this change. It includes
parse-negative grammar tests, not just runtime feature tests. Results change
from 40 pass / 35 fail to 53 pass / 22 fail: 13 improve and no former pass
regresses. All 22 failures remain visible, including eight variants whose original
propertyHelper.js cannot initialize (#467), class execution, eval and other
unimplemented behavior. These results do not establish full global-code conformance.


## Hosted x86_64 global-declaration comparison

[Run 40393438313](https://github.com/ieee0824/temiya/actions/runs/40393438313)
completed on Ubuntu 24.04, four logical CPUs. The
[provenance record](../benchmarks/global-declarations-hosted-provenance.json) retains
build identity and the original artifact reference. Both
[before/after](../benchmarks/global-declarations-hosted-beforeafter.json) and
[Boa](../benchmarks/global-declarations-hosted-boa.json) use the original eleven
workloads, 15 timed samples, one checked warmup and three alternating pairs.
Each mode has 66 successful runs and 1056 validated values. They run serially
after all builds, with the same compiler and existing 512 MiB/180-second guards.

The measured revision is `6e00625b91421dd5aabf821bad6ce9b489c77756`;
the baseline is `6f48fea38728cde271dccf354c95326bf1d1221e`. The measured
runtime is identical to code revision `71c277c0963990c6b6b44951435789d029f5f169`.
Later result-only commits do not modify executable source, worker scripts,
dependencies or the measurement workflow. Architectures are reported separately.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 0.98–1.01× | 3.33–4.65× |
| prop-mono | 0.99–1.05× | 2.40–2.77× |
| prop-mega | 1.01–1.02× | 2.24–2.26× |
| call | 1.00–1.05× | 2.32–2.37× |
| closure-alloc | 0.99–1.03× | 1.78–1.95× |
| object-alloc | 1.00–1.05× | 1.81–2.12× |
| string-concat | 0.99–1.01× | 2.96–3.02× |
| array | 0.99–1.02× | 2.11–2.18× |
| primitive-string-property | 1.01–1.02× | 2.66–3.30× |
| primitive-string-method | 1.00–1.05× | 2.04–2.46× |
| proto-method | 1.01–1.03× | 2.13–2.18× |

All 33 before/after pairs remain within the noise threshold; no throughput
speedup or regression beyond that threshold is established on this runner.
All 33 Boa pairs still find Temiya slower beyond the threshold. Both all-eleven
speed gates remain false. Correct global declaration behavior does not satisfy the Boa goal
or complete #307/#427.
