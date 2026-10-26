# Bytecode closures (work in progress, #292)

Function expressions and arrows now create Object callables whose bodies execute
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

#292 remains open. Function declarations and hoisting, for-let per-iteration
records, default/rest/destructuring parameters, mapped sloppy arguments and the
full arguments descriptor/prototype contract, async/generators, inferred names,
original pinned Test262 results, latest CI/security and performance validation
remain incomplete. Unsupported source forms fail compilation explicitly.
`compile_function_body` retains its existing caller-prepared no-parameter API.
Engine continues to run the AST backend (#307). Broader builtin limitations,
including missing BigInt.prototype.valueOf, are separate from correct wrapper
slot/prototype creation. No whole-function conformance or speed gain is claimed.
