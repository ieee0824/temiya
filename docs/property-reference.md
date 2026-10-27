# Property Reference execution

The explicit bytecode API now lowers ordinary and computed member reads,
simple member assignment, property deletion and optional member chains.
`BytecodeCompiler::compile` followed by `VirtualMachine::execute_in_realm`
executes those instructions through the runtime's real Reference and object
internal methods. `Engine` continues to use AST execution. Object/array literals,
calls/construct, function creation, compound assignment and update lowering
remain separate compiler work.

`GetProperty` consumes a base/key and pushes GetValue. `ResolveProperty` consumes
the same operands but retains their Reference before the assignment RHS;
`StoreReference` performs PutValue later. `DeleteReference` consumes the Reference
without fetching its property value. Runtime errors release transient stacks.
Verifier joins track retained Reference identities alongside scopes and operand
depth. Invalid property operands, missing scopes, Reference underflow and bad
nullish branch targets fail before execution.

AST and VM share `Reference::property_in_realm`. Object-valued computed keys
retain the Realm's well-known ToPrimitive symbol without retaining a Realm in
the Reference. JavaScript key conversion requests the string hint at consumption;
primitive and Symbol keys retain their normal conversion and identity. Existing
bootstrap Rust conversion-hook precedence is preserved; this does not complete
all wrapper/cross-Realm coercion work in #176.

Computed key expressions run before a null/undefined base fails. For assignment,
the RHS runs before the property write rejects a null base; the key object's
conversion method is not called once base conversion fails. References retain
the original base while RHS callbacks mutate variables. These orders follow
[EvaluatePropertyAccessWithExpressionKey](https://tc39.es/ecma262/2026/multipage/ecmascript-language-expressions.html#sec-evaluate-property-access-with-expression-key)
and are asserted with actual JavaScript getters and conversion methods.

Property deletion propagates Proxy trap exceptions and throws for a rejected
strict deletion. String primitive own length/index properties reject deletion;
other primitive deletes still convert the key. Primitive prototype setters get
the original primitive receiver, while string own immutable properties block a
write. Parentheses preserve an ordinary Reference, including method receivers
and deletion; an optional-chain short circuit becomes undefined at a grouping
boundary. Contiguous optional members skip remaining key expressions on nullish
bases, and optional deletion short circuits to true.

`LoadThis` resolves the supplied lexical this context. Script execution uses the
Realm's chosen global-this value; a caller-prepared function Environment supplies
its own this binding. The caller still owns function frame creation and argument
instantiation; this does not complete #291. BigInt literal constants permit BigInt
computed keys without claiming all operator lowering in #287.

Integration tests compare actual AST and VM execution for accessors, Symbol keys,
Proxy receivers and rejected sets, strict failures, inherited primitive setters,
nested References, reentrant VM getters, optional boundaries, chosen global-this,
and prepared function receivers. The public `backend_scope` example executes
`this.answer=42; this?.answer` through the VM.

## Upstream coverage remains incomplete

The ignored `pinned_test262_property_audit` executes original whole fixtures and
original sta/assert/includes from the pinned Test262 revision. It independently
attempts bytecode compilation; unsupported code is recorded without falling back
to AST. The audit exits unsuccessfully while any selected program remains
unsupported or fails. It is not an automatic green conformance gate.

The selected original programs require calls, functions, object syntax or other
unlowered operators. Consequently the explicit bytecode programs have no upstream
passes yet, despite the direct API integration tests. #288 remains open. The
next call/frame/function lowering work must enable those complete programs and
rerun this gate; parse-only checks or stripped assertions are not substitutes.
An AST failure caused by missing eval is retained independently.

```sh
TEST262_DIR=target/test262 TEST262_REPORT=property-audit.json cargo test --locked --all-features --test bytecode_properties pinned_test262_property_audit -- --ignored --nocapture
```

Performance is separate evidence: shared AST Reference changes affect the path
used by the embedding Engine. The original eleven omoikane inputs and existing
15-sample/three-pair result/noise/resource guards remain the comparison protocol.
Neither explicit-VM functionality nor this incomplete upstream audit establishes
speed superiority over Boa.

The [recorded audit](../test262/property-reference-audit.json) retains all 11
files / 19 variants and compiler/fixture identities. AST changes from 14 pass /
5 fail to 17 pass / 2 fail: the three strict-deletion cases improve; the two
missing-eval failures remain. Bytecode has zero passes and 19 unsupported whole
programs in both runs. The audit command exits 101. Those are separate from the
direct API regression tests; no unsupported program is included in a pass count.


## Hosted x86_64 property-Reference comparison

[Run 40404879412](https://github.com/ieee0824/temiya/actions/runs/40404879412)
completed on Ubuntu 24.04, four logical CPUs. The
[provenance record](../benchmarks/bytecode-properties-hosted-provenance.json) retains
build identity and the original artifact reference. Both
[before/after](../benchmarks/bytecode-properties-hosted-beforeafter.json) and
[Boa](../benchmarks/bytecode-properties-hosted-boa.json) use the original eleven
workloads, 15 timed samples, one checked warmup and three alternating pairs.
Each mode has 66 successful runs and 1056 validated values. They run serially
after all builds, with the same compiler and existing 512 MiB/180-second guards.

The measured revision is `85d262ed7ce00e3b184678d30c88d55557b2050f`;
the baseline is `c34c797cbb717b96efb3c952c9655baa38637c41`. The measured
runtime is identical to code revision `51174e5a69caa57961650d3083d8b369247fc1b7`.
Later result-only commits do not modify executable source, worker scripts,
dependencies or the measurement workflow. Architectures are reported separately.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 1.04–1.07× | 3.38–3.53× |
| prop-mono | 1.03–1.05× | 2.45–2.55× |
| prop-mega | 1.02–1.05× | 2.27–2.31× |
| call | 1.00–1.02× | 2.36–2.55× |
| closure-alloc | 1.00–1.02× | 1.83–1.91× |
| object-alloc | 1.00–1.04× | 1.73–1.77× |
| string-concat | 0.98–1.00× | 3.03–3.06× |
| array | 1.02–1.04× | 2.20–2.23× |
| primitive-string-property | 1.04–1.07× | 2.72–2.77× |
| primitive-string-method | 1.04–1.07× | 1.98–2.08× |
| proto-method | 0.99–1.01× | 2.18–2.26× |

All 33 before/after pairs remain within the noise threshold; no throughput
speedup or regression beyond that threshold is established on this runner.
All 33 Boa pairs still find Temiya slower beyond the threshold. Both all-eleven
speed gates remain false. Correct property Reference behavior does not satisfy the Boa goal
or complete #307/#427.
