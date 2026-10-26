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
