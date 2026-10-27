# Portable bytecode

`temiya::bytecode::Bytecode` is a pointer-free stack IR. Constants contain only
portable ECMAScript primitives and immutable scope descriptors; instruction operands are fixed-width `u32`
indices, and branch operands are absolute instruction indices. Runtime objects,
native addresses, and platform-sized operands are intentionally excluded.

| Instruction | Stack effect | Validation |
| --- | ---: | --- |
| `LoadConstant(i)` | `+1` | valid non-metadata constant |
| `EnterScope(i)`, `ExitScope` | `0` | valid scope descriptor and nesting |
| `LoadBinding {scope,slot}`, `LoadName(i)` | `+1` | active scope and valid slot/string name |
| `InitializeBinding`, `StoreBinding`, `StoreReference` | `-1` | one input and valid binding/reference |
| `GetProperty {strict}` | `-1` | base/key and active scope |
| `ResolveProperty {strict}` | `-2` | base/key; retain Reference |
| `DeleteReference` | `+1` | retained Reference |
| `LoadThis` | `+1` | active lexical this context |
| `JumpIfNullish(target)` | `-1` | one input and valid target |
| `LoadLocal(i)` | `+1` | `i < local_count` |
| `StoreLocal(i)` | `-1` | one input and valid local |
| `Duplicate` | `+1` | one input |
| `Pop` | `-1` | one input |
| `Add`, `Subtract`, `Multiply`, `Divide`, `StrictEqual` | `-1` | two inputs, one result |
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
arithmetic and strict equality, conditional expressions, blocks,
`if`, `while`, `return`, and `throw`) and immediately verifies its output.
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
undefined. Parameters, async/generators, call/frame creation, closure capture
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
