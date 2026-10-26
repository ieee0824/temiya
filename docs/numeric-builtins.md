# Numeric builtin boundaries (#226)

`Number.isSafeInteger` accepts only Number values, without coercing its argument,
and checks finite integral values through ±(2^53−1). Existing Number constants
retain nonwritable, nonenumerable, nonconfigurable descriptors; integration tests
verify all eight constants and signed-zero/NaN/infinity/safe-integer boundaries.

`BigInt.prototype.valueOf` and `toString` accept a BigInt primitive or an object
with an actual BigInt internal slot, including a foreign-Realm wrapper. A plain
object, prototype-derived object, Proxy, or the intrinsic prototype itself does
not acquire the brand. `toString` checks the receiver before converting radix,
uses Realm-aware number-hint conversion and ToIntegerOrInfinity, accepts 2–36,
and propagates conversion errors. Omitted/undefined radix means decimal.
Both methods and isSafeInteger expose Function.prototype and ordinary builtin
name/length/property descriptors. The BigInt prototype exposes its toStringTag.

`Object(value)` and direct `new Object(value)` now retain primitive wrapper data
and the corresponding intrinsic prototype, including BigInt and Symbol. Ordinary
Object inputs preserve their identity. This does not complete custom-newTarget
Object construction or missing Boolean/Symbol prototype APIs. Explicit Number
construction accepts BigInt, while ToNumber elsewhere still rejects it. Explicit
BigInt construction accepts finite integral Numbers using their exact binary64
integer value, not their shortest decimal spelling; fractions and infinities
throw RangeError. Both constructors use Realm-aware primitive conversion.

The public Engine and native API tests exercise real method calls, descriptors,
abrupt completion order, radix callbacks and foreign-Realm objects. Original
Test262 files and their original harness are also run without stripping or
skipping; shared harness/runtime failures remain visible. This does not claim
full Number/BigInt/Symbol compatibility or fulfillment of the Boa speed goal.

Reference: [ECMAScript 2026 numeric builtins](https://tc39.es/ecma262/2026/multipage/numbers-and-dates.html#sec-bigint.prototype.tostring).
