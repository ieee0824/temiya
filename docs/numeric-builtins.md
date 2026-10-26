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

## Fixed original upstream audit

All files in the pinned Test262 directories for BigInt.prototype.toString,
BigInt.prototype.valueOf and Number.isSafeInteger are selected: 31 files and
62 strict/sloppy variants. The original AST gate advances from **4 pass / 58
fail** to **40 pass / 22 fail**, identically on stable and Rust 1.85.0. No previous
pass regresses and no case is skipped. The same audit function is copied into
the unchanged baseline runtime; fixture blob IDs, all outcomes and transitions
are retained in [numeric-builtins-audit.json](../test262/numeric-builtins-audit.json).

All four opt-in runs still exit 101. Eighteen variants fail while evaluating the
original propertyHelper.js before the test program (#467); two require BigInt/Number
relational comparisons not yet merged from #473, and two require the callable Symbol constructor.
These are recorded as failures. Normal Engine/host tests independently verify
the actual descriptors, real Symbol values, wrappers, radix order/errors and
cross-Realm branding. They do not replace the failing original upstream cases.
Normal stable/MSRV all-target suites pass **365 tests**, with 9 ignored; final
focused tests also pass on both toolchains. Formatting and all-target/all-feature
Clippy with warnings denied pass. This source needs its own hosted CI/security
and before/after plus actual pinned omoikane Boa measurements before review.

Reference: [ECMAScript 2026 numeric builtins](https://tc39.es/ecma262/2026/multipage/numbers-and-dates.html#sec-bigint.prototype.tostring).
