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


## Hosted numeric builtin comparison

[Run 40423209543](https://github.com/ieee0824/temiya/actions/runs/40423209543)
completed on Ubuntu 24.04, AMD EPYC 9V45 96-Core Processor, four logical CPUs,
with the same Rust 1.99.1 compiler. [Provenance](../benchmarks/numeric-builtins-hosted-provenance.json),
[before/after](../benchmarks/numeric-builtins-hosted-beforeafter.json) and
[actual pinned omoikane Boa](../benchmarks/numeric-builtins-hosted-boa.json)
retain the original eleven workloads and oracles, 15 timed samples plus a checked
warmup, three alternating pairs and serial execution with 512 MiB/180-second
guards. Each mode has 66 successful runs and 1056 validated values.

The frozen measured revision is `1384ea32e05f25709e97e681a59f2f8e168f5ad0`,
with executable source `a666b920ec2713564ae1044f740fa8c1a377773f`,
versus baseline `c34c797cbb717b96efb3c952c9655baa38637c41`. The later parent
integration `a399ee8743dafc4d2d64548df39fe9d9a952db98` includes merged VM
foundations and binary operators and is **not** executable-equivalent to the measured source. These
reports isolate the earlier numeric additions; final combined-main throughput
requires a separate comparison. Report-only changes after a399 change no runtime.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 0.932–0.964× | 3.245–4.571× |
| prop-mono | 0.986–1.008× | 2.243–2.417× |
| prop-mega | 0.974–0.986× | 2.208–2.570× |
| call | 0.921–0.939× | 2.261–2.748× |
| closure-alloc | 0.943–0.945× | 1.656–1.747× |
| object-alloc | 0.953–1.104× | 1.700–1.999× |
| string-concat | 0.979–0.992× | 2.953–3.003× |
| array | 0.969–0.994× | 2.043–2.209× |
| primitive-string-property | 0.961–0.995× | 2.591–3.295× |
| primitive-string-method | 0.920–0.968× | 2.039–2.365× |
| proto-method | 0.949–0.993× | 2.138–2.604× |

Thirty-two before/after pairs are within the fixed noise threshold. Object-alloc
pair 1 takes **1.103591× the baseline time** (10.36% slower), with a 0.1 threshold;
its other pairs are 0.952615×
and 0.967431×. The flagged comparison remains a regression observation; the
three pairs do not establish a consistent directional change. No regression-free
claim is made. The combined-source follow-up must retain and investigate this
workload. All 33 Boa comparisons find Temiya slower beyond threshold. Both speed
gates remain false; compatibility completion is separate from the Boa goal.


After integration with binary operators, normal stable/MSRV all-target tests each
pass **429 tests**, with 12 ignored. The unchanged original 16-file/31-variant
binary selection now has **31 AST pass / 0 fail**, and **21 VM pass / 0 fail /
10 compile-unsupported** on both toolchains. BigInt wrapper-related executed
failures are resolved; the VM still rejects the same original prerequisite syntax.
The unchanged numeric selection remains **40 pass / 22 fail**; its a-z case
now advances beyond BigInt/Number comparison and reaches the still-missing
String.fromCharCode call. Its failure remains a failure. All four full opt-in
runs still exit 101 because unsupported programs/shared prerequisites remain.
These combined-source outcomes do not revise the older frozen audit or speed
reports. A final combined-main comparison is tracked in #475.
