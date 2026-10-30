# Portable bytecode

Engine defaults to whole-Script Auto selection: supported Scripts use verified bytecode and unsupported Scripts use AST before any observable execution. In an Auto AST Script, eligible ordinary/class function bodies can independently use the verified bytecode function runtime; unsupported bodies remain AST, and errors are never retried. Explicit Ast keeps AST bodies and explicit Bytecode still rejects an unsupported whole Script. This does not enable a JIT or hot-loop tier. Earlier sections describe historical implementation stages, including the former AST default. The pre-GC default API passed all 33 original-eleven Boa comparisons on both recorded x64 and ARM64 machines; see [integration scope and evidence](auto-backend-integration.md). Runtime changes require fresh performance qualification.

Eligible function bodies can contain class expressions through verified `CreateClass`
metadata. The VM invokes the shared class evaluator with the current environment;
class declarations and top-level class expressions still require AST selection.
This bridge preserves class name inference in variable initializers and simple
identifier assignments, captures per-iteration environments, and does not enable
class fields, private elements or general class bytecode lowering. Anonymous
class object-property values and logical assignments require preexecution AST
fallback. See [class execution](classes.md) for the bridge's this/new.target and
ownership contracts.


## Guarded numeric returns

Verification derives a private execution plan for straight-line function bodies
that read parameters or captured numbers, perform numeric operations, and return
a primitive result. Each call checks the current values; non-number parameters,
uninitialized bindings, observable lookups, arguments objects, and unsupported
instructions use the normal VM. Capture guards inspect only declarative/function
binding cells and never call getters or coercion hooks. Arithmetic uses the
existing Number operators, retaining NaN, signed zero and evaluation order.

The plan is immutable code metadata with no runtime handles or context owners.
It uses a bounded operand stack; larger bodies retain normal VM execution.
Eligible calls need no managed function frame. Function objects, descriptors,
prototype identity, captured bindings and call-depth checks keep their existing
behavior. Tests compare AST/Bytecode results and observable fallback effects,
and verify that cached code does not retain an otherwise unreachable heap graph.


`temiya::bytecode::Bytecode` is a pointer-free stack IR. Constants contain only
portable ECMAScript primitives and immutable scope/function descriptors; instruction operands are fixed-width `u32`
indices, and branch operands are absolute instruction indices. Runtime objects,
native addresses, and platform-sized operands are intentionally excluded.

| Instruction | Stack effect | Validation |
| --- | ---: | --- |
| `LoadConstant(i)` | `+1` | valid non-metadata constant |
| `EnterScope(i)`, `ExitScope` | `0` | valid scope descriptor and nesting |
| `CloneLoopBindings` | `0` | active let-only block, no hoisted functions; keep scope entry identity and clone its environment |
| `LoadBinding {scope,slot}`, `LoadName(i)` | `+1` | active scope and valid slot/string name |
| `InitializeBinding`, `StoreBinding`, `StoreReference` | `-1` | one input and valid binding/reference |
| `GetProperty {strict}` | `-1` | base/key and active scope |
| `ResolveProperty {strict}` | `-2` | base/key; retain Reference |
| `DeleteReference` | `+1` | retained Reference |
| `LoadReference` | `+1` | retained Reference; reads without consuming it |
| `DiscardReference` | `0` | consumes retained Reference without a write |
| `BeginCall`, `BeginConstruct` | `-1` | capture callee in pending invocation |
| `BeginReferenceCall` | `0` | consume Reference; capture GetValue and receiver |
| `LoadReferenceCallTarget` | `+2` | consume Reference; push receiver/callee once |
| `AsCallTarget` | `+1` | value becomes undefined-receiver/callee pair |
| `BeginMethodCall` | `-2` | capture receiver/callee pair |
| `AppendArgument`, `SpreadArguments` | `-1` | exactly one value above invocation baseline; spread also needs Realm scope |
| `Invoke` | `+1` | balanced invocation at baseline |
| `CreateFunction(i)` | `+1` | active scope and verified function template |
| `LoadNewTarget` | `+1` | active lexical context |
| `UpdateReference {increment,prefix}` | `+1` | consume Reference; Number/BigInt update and prefix/postfix result |
| `TypeOfReference` | `+1` | consume Reference; special-case unresolvable name |
| `TypeOf`, `UnaryNumeric(op)` | `0` | one input; shared typeof/numeric conversion |
| `LoadThis` | `+1` | active lexical this context |
| `JumpIfNullish(target)` | `-1` | one input and valid target |
| `LoadLocal(i)` | `+1` | `i < local_count` |
| `StoreLocal(i)` | `-1` | one input and valid local |
| `Duplicate` | `+1` | one input |
| `Pop` | `-1` | one input |
| `Add`, `Subtract`, `Multiply`, `Divide`, `StrictEqual` | `-1` | two inputs, one result |
| `Binary(op)` | `-1` | two inputs; generic arithmetic/comparison/bitwise/in/instanceof |
| `LogicalNot` | `0` | one input, one result |
| `Jump(target)` | `0` | target is an instruction |
| `JumpIfFalse(target)` | `-1` | one input and valid target |
| `Return`, `Complete`, `Throw` | `-1` | exactly one value, terminal |

`Bytecode::verify` performs a work-list traversal before dispatch is allowed.
It validates pool and frame operands, rejects stack underflow, requires equal
stack depth and active scopes at control-flow joins, rejects falling beyond the code,
rejects unreachable instructions, and computes the maximum stack allocation.
Only the resulting `VerifiedBytecode` is intended for execution by the VM.

`BytecodeCompiler` lowers the baseline AST subset (primitive literals, scoped bindings,
assignment, property get/set/delete, optional member chains, BigInt literals,
binary/unary/update and compound operators, logical short circuit, conditional expressions, blocks,
`if`, `while`, classic `for`, `do-while`, `break`, `continue`, labels,
`return`, and `throw`) and immediately verifies its output.
Unsupported syntax returns a source-spanned `CompileError` rather than falling
back silently. `VirtualMachine` accepts only `VerifiedBytecode`, evaluates
operands left-to-right, uses the runtime's ECMAScript conversions, and returns
the same `Completion` variants as the AST statement evaluator.

## Experimental baseline JIT

The `jit` Cargo feature enables `BaselineJit`; it is absent from default builds
and `libc` is an optional dependency activated only by that feature. On
x86-64 Unix, verified straight-line numeric bytecode is lowered to SSE2 machine
code. Executable memory follows a W^X transition: bytes are written to an RW
mapping, then the mapping becomes RX before execution. Unsupported platforms,
instructions, completions, or register pressure return typed `JitError`s so the
embedder can explicitly select the bytecode interpreter fallback. This is an
experimental baseline backend, not an optimizing tier.

## Environments and binding slots

Scope descriptors in the constant pool contain a category (`Global`, `Function`,
`Block`), code strictness and ordered names with `Var`, `Let`, or `Const` policies.
They contain no runtime Environment, closure, or object. `LoadConstant` cannot
load metadata. `EnterScope(descriptor)` identifies its activation by the entry
instruction PC; `ExitScope` discards that activation. `LoadBinding`,
`InitializeBinding` and `StoreBinding` address an active entry PC and a slot in
its descriptor. These slots currently map to the existing named Environment
operations; this is not a direct-indexed local-storage optimization.

`var` declarations are hoisted and initialized to undefined at entry. A `var`
without an initializer preserves its value. Lexical declarations stay in the
TDZ until their initializer (or undefined for `let` without one) is evaluated.
`const` remains immutable in both strict and sloppy code. Blocks instantiate
fresh declarative records, including on each loop iteration. Runtime errors and
return/throw completions discard transient VM block frames.

`VirtualMachine::execute_in_realm` uses the supplied Realm's persistent global
Environment: `var` uses the global object record; `let`/`const` use its declarative
record. Declaration conflicts and global-object restrictions are checked before
creating any binding. `LoadName`/`ResolveName` resolve names not statically declared
in this code through the actual Environment chain; an unresolved read or strict
assignment raises ReferenceError. Sloppy unresolved assignments use the Realm's
global object. `ResolveName` retains the actual Reference before evaluating the
right operand; `StoreReference` then applies PutValue without re-resolving a name
after coercion callbacks. Nested assignments retain references in order, and
verification checks reference-stack identities at joins and rejects underflow. The older `execute` creates a fresh Realm for scoped bytecode;
legacy unscoped manually constructed IR keeps its original raw-local execution.

`BytecodeCompiler::compile_function_body` and
`VirtualMachine::execute_in_function` execute a body against a caller-prepared
Function Environment. The caller owns the real function object, outer chain,
`this`, parameters and arguments. Existing var bindings in that frame are
preserved; body lexical bindings occupy a separate block. Fallthrough returns
undefined. Function expressions/arrows now use the separate template-backed
creation path described in bytecode-closures.md. Parameters, async/generators, closure capture
lowering and Engine backend selection remain work in #291/#292/#307. Supplying
a global or declarative record as a function frame fails explicitly.

Verification tracks both operand depth and active entry identities at every
control-flow edge. Joins must agree on both; inactive scopes, invalid slots,
var initialization opcodes, duplicate descriptor names, invalid nesting, block
var descriptors, metadata-as-value and name operands of the wrong type are
rejected. `Complete` requires scopes to be exited; return/throw unwind them.
The numeric JIT accepts only empty global scope entry/exit around its existing
numeric subset. Binding/name operations and nonempty scopes remain unsupported.

Tests compare AST and VM shadowing, TDZ reads/writes, const assignment, hoisting,
loop blocks and exception unwinding. Persistent global-object versus lexical
record behavior is checked directly. The AST Script global-record connection
tracked in #466 also has paired state/error tests in `tests/global_declarations.rs`;
Engine backend selection remains separate work in #307. Scoped `+`
shares the AST's Realm-aware JavaScript coercion helper and evaluation order.

The pinned Test262 revision in `src/test262.rs` supplies four original runtime
negative files: `language/statements/{let,const}/global-use-before-initialization-in-{prior-statement,declaration-statement}.js`.
All eight strict/sloppy variants must parse and compile before both AST and VM
raise ReferenceError. The [recorded focused results](../test262/bytecode-scope-regressions.json) retain
fixture hashes, compiler identities and all 32 AST/VM executions across both
local toolchains. CI runs them on stable and MSRV; no unsupported compilation
is counted as a pass. This is a focused regression set, not full conformance.

```sh
cargo run --locked --example backend_scope --all-features
scripts/test262
TEST262_DIR=target/test262 cargo test --locked --all-features --test bytecode_scopes pinned_test262_bytecode_scope_tdz -- --ignored --nocapture
```

The embedding Engine still uses the AST interpreter. This work establishes
scope semantics needed by future bytecode integration; it does not establish a
speedup on omoikane or satisfy the Boa speed gate.

Global declaration checks follow [GlobalDeclarationInstantiation](https://tc39.es/ecma262/2026/multipage/ecmascript-language-scripts-and-modules.html#sec-globaldeclarationinstantiation) for the supported var/lexical subset. Function declarations, Annex B rules and exhaustive exotic-global behavior remain separate work.


## Hosted x86_64 Engine comparison after scope support

[Run 40388707981](https://github.com/ieee0824/temiya/actions/runs/40388707981)
completed on Ubuntu 24.04, four logical CPUs. The
[provenance record](../benchmarks/bytecode-scopes-hosted-provenance.json) retains
build identity and the original artifact reference. Both
[before/after](../benchmarks/bytecode-scopes-hosted-beforeafter.json) and
[Boa](../benchmarks/bytecode-scopes-hosted-boa.json) use the original eleven
workloads, 15 timed samples, one checked warmup and three alternating pairs.
Each mode has 66 successful runs and 1056 validated values. They run serially
after all builds, with the same compiler and existing 512 MiB/180-second guards.

The measured revision is `7b6955b4c76ef3eab4a302199e339ac0cb8c6b57`;
the baseline is `d161a8d4c0d100faa1954a99396ea79d39f02d6c`. The measured
runtime is identical to code revision `7b6955b4c76ef3eab4a302199e339ac0cb8c6b57`.
Later result-only commits do not modify executable source, worker scripts,
dependencies or the measurement workflow. Architectures are reported separately.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 0.97–1.01× | 3.49–3.60× |
| prop-mono | 0.98–1.00× | 2.73–2.75× |
| prop-mega | 1.00–1.04× | 2.45–2.48× |
| call | 0.97–0.98× | 2.30–2.40× |
| closure-alloc | 0.98–1.02× | 1.73–1.90× |
| object-alloc | 0.97–1.00× | 1.85–1.88× |
| string-concat | 0.96–0.99× | 3.12–3.25× |
| array | 1.01–1.02× | 2.31–2.33× |
| primitive-string-property | 0.99–1.01× | 2.89–2.96× |
| primitive-string-method | 0.98–0.99× | 2.16–2.18× |
| proto-method | 0.97–0.99× | 2.28–2.30× |

All 33 before/after pairs remain within the noise threshold; no throughput
speedup or regression beyond that threshold is established on this runner.
All 33 Boa pairs still find Temiya slower beyond the threshold. Both all-eleven
speed gates remain false. Bytecode scope support does not satisfy the Boa goal
or complete #307/#427.

Property Reference execution and its current upstream frontend gaps are described
in [property-reference.md](property-reference.md). #288 remains open until the
original upstream programs can be validated through the complete bytecode path.

## Invocation work in progress (#291)

Ordinary and optional calls, computed and parenthesized method calls, and construction now
lower to pending invocation frames. A frame captures the callee and receiver
before evaluating arguments, appends each argument in order, then dispatches the
existing Object call/construct API. Construction uses the captured constructor
as newTarget. Callability is checked after argument evaluation in both AST and
VM paths. Native callbacks and existing AST-backed JavaScript function objects
are callable; function expressions/arrows additionally create actual VM-backed
objects. Ordinary function declarations additionally use scope-instantiation metadata. Engine continues to use the AST backend.

Pending frames own their argument and receiver roots separately from operand
storage. Verification tracks frame entry identity and operand baseline across
joins, rejects missing/unbalanced frames, and prevents instructions from consuming
values below a pending invocation's baseline. Runtime errors drop all frames;
nested VM runs use separate storage. Object call and construct share a per-thread
limit of 32 nested callbacks, including AST functions and reentry. Exceeding this
conservative Rust-stack limit returns RangeError and releases the nesting budget
on unwind. This is an observable current implementation limit, not a claim of
unbounded JavaScript recursion or an iterative VM function stack.

Optional chains lower as one contiguous member/call path. A nullish branch
releases the retained base or receiver/callee pair, skips remaining keys and
arguments, and yields undefined (true for deletion). Parentheses terminate
short-circuit propagation while retaining a successful method's receiver.
Reference GetValue occurs once before the optional-call check. Falsy non-nullish
callees still evaluate arguments and fail callability checks normally. Local
AST/VM comparisons cover mixed chains, grouped calls, skipped arguments and
error precedence, following [ECMA-262 2026 optional chains](https://tc39.es/ecma262/2026/multipage/ecmascript-language-expressions.html#sec-optional-chains).

Spread arguments now consume the actual synchronous iterator protocol in both
AST and VM argument lists: GetMethod(@@iterator), call with the iterable receiver,
capture next once, then read done before value for each object result. Existing
array-like objects without @@iterator are rejected; overridden iterator methods
are honored. Native and JavaScript callbacks, getters and thrown values use the
same Object API as ordinary calls. Standard Array values/@@iterator and String
@@iterator are installed with shared branded next methods, Realm-local prototypes,
and identity @@iterator inheritance. String iteration emits code points while
preserving lone UTF-16 surrogates. Array values observe dynamic length, holes,
reentrant getters and index advancement before element Get; exhausted iterators
release their target. The iterator instance stores boxed private state rather
than a closure capturing itself. Its optional slot adds one pointer to ObjectState.

Argument lists currently have an implementation limit of 65,536 values (ordinary
and spread combined). Overflow returns RangeError, closes a still-active iterator,
and drops pending invocation roots; the original overflow takes precedence over
return getter/call failures. Iterator-step/done/value failures propagate without
an extra return call, as required by ArgumentListEvaluation. These guards and
added intrinsic objects are included in the hosted comparison recorded below.

This implements argument spread and Array values/String iterator prerequisites,
not full iterator/builtin conformance: Array keys/entries, TypedArray-specific
iteration and detached-buffer behavior, Map/Set/generator iterator exposure,
Iterator constructor/helpers, cross-Realm symbol identity, and the older AST
array-literal spread algorithm remain separate unfinished areas. The required original Test262 call/construct suite has
not yet been run for this work; local integration tests do not satisfy that gate.
Issue #291 remains incomplete. The added shared Object-call guard changes the AST
hot path; the hosted call-frame comparison below records the required fresh
comparative evidence. #469's measurement does not include this work. No speed improvement is claimed.

Function expression/arrow lowering is in progress in #292; see [bytecode closures](bytecode-closures.md) for actual execution, ownership, tests and unfinished gates. The full parameter/arguments contract and sloppy block declarations remain incomplete.

Operator lowering and computed-Reference conversion fixes are in progress in #287;
see [unary/update operators](bytecode-operators.md) and
[binary/compound operators](bytecode-binary.md) for execution semantics and unfinished gates.
Unary/update lowering and computed-Reference conversion fixes are in progress in #287; see [bytecode operators](bytecode-operators.md) for execution semantics and unfinished gates.
## Hosted x86_64 call-frame and iterator comparison

[Run 40409675780](https://github.com/ieee0824/temiya/actions/runs/40409675780)
completed on Ubuntu 24.04, four logical CPUs. The
[provenance record](../benchmarks/bytecode-calls-hosted-provenance.json) retains
build identity and the original artifact reference. Both
[before/after](../benchmarks/bytecode-calls-hosted-beforeafter.json) and
[Boa](../benchmarks/bytecode-calls-hosted-boa.json) use the original eleven
workloads, 15 timed samples, one checked warmup and three alternating pairs.
Each mode has 66 successful runs and 1056 validated values. They run serially
after all builds, with the same compiler and existing 512 MiB/180-second guards.

The measured revision is `c0f163b21ab5c75ada809f6fb8de484d48568405`;
the baseline is `85d262ed7ce00e3b184678d30c88d55557b2050f`. The measured
runtime is identical to code revision `c0f163b21ab5c75ada809f6fb8de484d48568405`.
Later result-only commits do not modify executable source, worker scripts,
dependencies or the measurement workflow. Architectures are reported separately.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 1.01–1.02× | 3.35–3.37× |
| prop-mono | 1.00–1.02× | 2.46–2.63× |
| prop-mega | 1.01–1.03× | 2.33–2.38× |
| call | 1.06–1.06× | 2.49–2.64× |
| closure-alloc | 1.02–1.03× | 1.81–1.86× |
| object-alloc | 1.01–1.03× | 1.70–1.76× |
| string-concat | 0.96–1.00× | 3.02–3.32× |
| array | 1.02–1.04× | 2.08–2.28× |
| primitive-string-property | 0.99–1.05× | 2.77–2.82× |
| primitive-string-method | 1.01–1.03× | 1.97–2.09× |
| proto-method | 1.04–1.05× | 2.33–2.48× |

All 33 before/after pairs remain within the noise threshold; no throughput
speedup or regression beyond that threshold is established on this runner.
All 33 Boa pairs still find Temiya slower beyond the threshold. Both all-eleven
speed gates remain false. Call-frame and iterator integration does not satisfy the Boa goal
or complete #307/#427.

## Loop control and per-iteration environments

Classic for, do-while, while, labelled blocks/loops and break/continue lower to
verified branches. Abrupt branches emit ExitScope for intervening lexical
environments. Chained labels share one target; continue selects an iteration
target, not a labelled block. Conditions and updates preserve normal source
evaluation order; unreachable back edges are not emitted after terminal bodies.

For a let head, CloneLoopBindings copies values into a fresh sibling declarative
environment before the first condition and before each increment. Existing
closures retain the previous environment. Scope entry PCs/slot descriptors stay
stable while the runtime environment is replaced. Const heads are not cloned.
The verifier rejects cloning a global/function scope, const bindings, or hoisted
functions. Source compilation also validates early errors in owned ASTs before
discarding unreachable code.

Script loop/if completion values initialize to undefined as required by the
[statement algorithms](https://tc39.es/ecma262/multipage/ecmascript-language-statements-and-declarations.html#sec-runtime-semantics-forbodyevaluation).
Empty statements preserve the accumulated value; expression initializers,
conditions and increments do not replace it. Compiled function bodies discard
unused statement completions and do not allocate a completion local. This
is separate from the Engine selection described below.

The public examples/backend_scope.rs demonstrates captured let bindings and
nested labelled continue. Six direct regression groups also cover const/TDZ,
initialization captures, terminal bodies, exact completion values, invalid
targets in unreachable owned ASTs, malformed clone instructions, and exceptions.

[Complete pinned audit](../test262/bytecode-loops-audit.json) retains the original
24 Test262 programs /46 variants: baseline4 pass/42 fail becomes35 pass/11 fail,
with31 new passes and no previous-pass regression. No test program is shortened,
replaced, skipped, or sent to AST when VM lowering fails. Original sta/assert
and include programs use the existing AST host harness.

The35 loop-target variants all pass. Eleven dependency cases remain failing:
eval(#305), array literals(#290), try/catch and sloppy block-function Annex B
semantics. The complete selected gate remains false and its full opt-in test
exits101. CI executes the same complete audit, then explicitly checks coverage,
all35 target variants, and known dependency diagnostics; those11 failures are
not counted as passes. Whole directory coverage, AST for-let correctness(#173),
Engine backend integration and overall Boa superiority remain open.


### Engine Script backend selection

`Engine::builder().script_backend(ScriptBackend::Bytecode)` compiles the entire
Script to verified bytecode before execution. Unsupported syntax returns an
`EmbedError::Bytecode` diagnostic without executing the Script. `Auto` selects
bytecode for a supported whole Script and falls back to AST otherwise;
`CompiledScript::backend()` reports the actual choice. The default remains
AST. Selection applies to Scripts compiled by that Engine, not modules or
functions previously created by the AST interpreter. A VM runtime exception
is propagated and never triggers AST re-execution.


### Array literals and iterable spread

[Array literal operations](array-literals.md) describe accumulation, holes,
iterator errors, operand checks and the scoped upstream observations.
### Object initializer data properties

See [object data literals](object-data-literals.md) for bytecode instructions,
key/value ordering, prototype setters, object spread, scoped upstream tests
and remaining method/HomeObject limits.

Private primitive-only function loops may keep var, let, and const slots outside
managed environments. This requires a whole-body proof excluding captures, calls,
property operations, arguments access, and escaping references, plus numeric
arguments. Lexical slots retain TDZ and const assignment checks; per-iteration
environment identities are omitted only when no observer can retain them. The
public caller-prepared frame API continues publishing all binding writes.
Empty block descriptors retain their verified scope entries while reusing the
parent environment, since they introduce no binding identity or declaration.
The existing explicit-Ast for-let header capture gap is tracked in #537; Auto's
verified function-loop path already retains the distinct iteration bindings.

Strict, non-arrow bytecode functions without an arguments object can execute a
verified straight-line body without creating an activation Environment. The
whole-body plan permits parameter/undefined-var reads, literal values, `this`,
named property references and primitive operators; it rejects captures, local
binding writes, closures, nonempty lexical scopes, calls, branches and
`new.target`. Other bodies retain the normal VM path. This optimization also
applies to eligible base constructors and methods inside Auto's AST fallback;
explicit Ast is unchanged.

Property reads/writes, updates and coercions use the existing runtime operations,
including reentrant getters/setters and their original receivers. Function,
newTarget, this, argument and outer-environment roots remain alive during these
callbacks. A runtime error propagates directly and never replays the body. The
public prepared-frame VM API continues to execute against its supplied frame.
Tests verify zero managed activation allocations, original accessor/coercion
order and thrown-value identity, source-Realm errors, BigInt updates and GC
inside nested accessor execution. No JIT or benchmark-specific recognition is
involved.

Fresh internal function frames can reuse per-iteration let binding identities
when a complete verified-body proof excludes closure creation, hoisted functions
and eval references/bindings. No retained Reference or pending invocation may
span a clone instruction. The opcode match is exhaustive, so new instructions
require a review. Variable writes and lexical checks still use ordinary
Environments; callbacks and property operations retain their usual semantics.
Bodies that can expose iteration cells and the public prepared-frame API retain
cloning. Tests cover escaped closures, closures returned by external factories,
abrupt loop control, constant managed-allocation growth and handwritten verified
code that holds a binding Reference across a clone.
