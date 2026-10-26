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
