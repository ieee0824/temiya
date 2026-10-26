# Addition coercion through the public Engine

The AST evaluator connects ordinary JavaScript objects used by `+` and `+=` to
realm-aware primitive conversion. It looks up `Symbol.toPrimitive`, invokes it
with the original receiver and the single `"default"` argument, and rejects a
non-callable method or an Object result. A null/undefined exotic method falls
back to callable `valueOf` and then `toString`, obtaining each method only when
needed. Getter and call exceptions retain their original throw value.

Both operands are evaluated before conversion starts. The left operand converts
before the right; both conversions finish before string/numeric processing.
Compound assignment reads its reference once and writes only after successful
conversion and addition. Existing Rust conversion hooks retain their embedding
contract and receive `PrimitiveHint::Default`. Primitive-only operands continue
directly to the existing arithmetic path without a realm symbol lookup.

This follows the project's adopted ECMAScript 2026 algorithms
[ToPrimitive](https://tc39.es/ecma262/2026/multipage/abstract-operations.html#sec-toprimitive)
and [ApplyStringOrNumericBinaryOperator](https://tc39.es/ecma262/2026/multipage/ecmascript-language-expressions.html#sec-applystringornumericbinaryoperator).
It does not complete the operator audit in #176. Other operators still use
their existing coercion paths. Intrinsic/wrapper support, overriding native
conversion hooks with JavaScript methods, cross-realm well-known Symbol identity,
and Object bytecode execution remain separate work.

`tests/addition_coercion.rs` exercises ordinary and exotic conversion, method
receivers, null/undefined methods, object/non-callable results, null prototypes,
Number/BigInt/String/Symbol results, exact thrown identity, operand/conversion
ordering, compound-assignment getters/setters, and existing embedding hooks.

Run the focused upstream selection with:

```sh
scripts/test262
TEST262_DIR=target/test262 cargo test --test addition_coercion \
  pinned_test262_addition_coercion -- --ignored --nocapture
```

It uses the original pinned Test262 source and harness, and runs the declared
strict/sloppy variants. The default six-file selection has 12 passing variants.
`TEST262_CASE` accepts comma-separated filenames from the addition directory;
`TEST262_REPORT` optionally writes all outcomes. Unsupported metadata is recorded
as skipped and prevents the selection from passing.

The [complete addition audit](../test262/addition-coercion-audit.json) at the same pin contains 48 files and 95
variants. The before/after audit improves from 31 passes/64 failures to
59 passes/36 failures, with zero skipped files and no previously passing variant
failing. The remaining failures include eval/isNaN exposure, Boolean and
BigInt/Symbol boxing, Date conversion, Function.prototype.toString, and Symbol
construction. They remain failures; this is not full addition conformance.
