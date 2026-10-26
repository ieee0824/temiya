# Bytecode unary/update integration (in progress, #287)

The explicit VM now lowers numeric unary plus/minus/bitwise-not, typeof, void,
and prefix/postfix increment/decrement, alongside the existing delete/logical-not
paths. Updates resolve one actual Reference before GetValue, perform ToNumeric,
write through that same Reference and return the numeric old/new value. BigInt
updates stay BigInt; postfix conversion of a numeric string returns a Number.
Typeof an unresolvable identifier returns undefined's type string, while a TDZ
binding still throws. Void evaluates its operand before discarding its value.

AST and VM share Realm-aware numeric unary/update coercion. Object operands use
JavaScript @@toPrimitive or ordinary valueOf/toString with the number hint;
existing bootstrap Rust conversion-hook precedence is preserved. Unary plus
rejects BigInt; negation/bitwise-not retain it. Errors propagate before a write.
The broader operator/wrapper/cross-Realm audit (#176/#226) remains incomplete.

GetValue now retains an object-valued computed key's resolved PropertyKey for
subsequent PutValue on that same Reference, following ECMA-262 2026's mutation of
[[ReferencedName]]. This also fixes repeated key coercion in the AST update and
compound paths. Only object-valued computed names allocate a boxed mutable cell;
ordinary environment/static/primitive names avoid dynamic borrows. Pure primitive
key conversions can be repeated as an observationally equivalent fast path.
No borrow survives user conversion/getter callbacks. The original key object is
released after successful conversion and before its getter; tests verify that
release and reentry into the same public Reference. Failed conversions do not
cache a result. Cloning copies the Reference record's conversion state.

Integration tests compile complete JS programs using actual Object descriptors
and JS conversion methods in both AST and VM, and test the public host Reference
API. They cover conversion/get/set order and receiver preservation, changing keys,
BigInt/numeric results, signed zero/NaN/infinity, typeof/TDZ, strict string writes,
const/Symbol/throw failures, and computed-key release/reentry. Fixed original
Test262 follow-up results are recorded separately from normal tests.

#287 remains open: the remaining binary and compound-assignment lowering,
short-circuit combinations and their full coercion audit, complete fixed upstream
gates, latest x86/security checks and fresh performance evidence are unfinished.
Unsupported source fails compilation explicitly; no AST fallback is used. Engine
still uses AST. Reference layout and shared numeric hot paths change, so prior
#470 measurements do not establish this version's performance. No speed gain or
Boa superiority is claimed.

Reference algorithms: [GetValue/PutValue](https://tc39.es/ecma262/2026/multipage/ecmascript-data-types-and-values.html#sec-getvalue)
and [update expressions](https://tc39.es/ecma262/2026/multipage/ecmascript-language-expressions.html#sec-update-expressions).
