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

## Fixed original follow-up audits

The unchanged original closure selection (12 files/17 variants) advances from
4 VM pass / 2 fail / 11 compile-unsupported to **6 pass / 2 fail / 9 unsupported**.
The unchanged property selection (11 files/19 variants) advances from
9 VM pass / 0 fail / 10 unsupported to **11 pass / 0 fail / 8 unsupported**.
Both toolchains agree, with no previously passing variant regressed. The newly
passing programs exercise named-function updates inside arrows and contiguous
optional-chain short circuiting around prefix updates. AST stays 10 pass / 7 fail
and 17 pass / 2 fail in the two selections, respectively.

[operator-followup-audit.json](../test262/operator-followup-audit.json) retains all
before/after stable/MSRV outcomes and transitions. Original host harness runs in
AST for both backends; each entire test program is independently compiled/executed
in the VM with no fallback/stripping. All four opt-in audits still exit 101; their
remaining failures/unsupported are not counted as pass or skip. This does not
complete #287, #288 or #292. Normal all-target stable/MSRV tests pass 413 tests,
with 10 ignored. Fresh performance measurements remain required.


## Hosted x86_64 unary/update and Reference comparison

[Run 40418195508](https://github.com/ieee0824/temiya/actions/runs/40418195508)
completed on Ubuntu 24.04, AMD EPYC 9V74 80-Core Processor, four logical CPUs. The
[provenance record](../benchmarks/bytecode-operators-hosted-provenance.json) retains
build identity and the original artifact reference. Both
[before/after](../benchmarks/bytecode-operators-hosted-beforeafter.json) and
[Boa](../benchmarks/bytecode-operators-hosted-boa.json) use the original eleven
workloads, 15 timed samples, one checked warmup and three alternating pairs.
Each mode has 66 successful runs and 1056 validated values. They run serially
after all builds, with the same compiler and existing 512 MiB/180-second guards.

The measured revision is `c84c88fb456c9b485a4c9350410efe29878968af`;
the baseline is `d30c341663cbbce9a85b5c72b42b2a103054fbdc`. The measured
runtime is identical to code revision `c84c88fb456c9b485a4c9350410efe29878968af`.
Later result-only commits do not modify executable source, worker scripts,
dependencies or the measurement workflow. Architectures are reported separately.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 1.03–1.06× | 3.72–3.83× |
| prop-mono | 1.00–1.02× | 2.81–2.82× |
| prop-mega | 1.03–1.07× | 2.59–2.65× |
| call | 1.01–1.04× | 2.44–2.53× |
| closure-alloc | 1.03–1.04× | 1.91–1.94× |
| object-alloc | 1.00–1.01× | 1.93–1.95× |
| string-concat | 1.03–1.05× | 3.38–3.46× |
| array | 0.97–1.01× | 2.40–2.43× |
| primitive-string-property | 1.02–1.03× | 3.10–3.14× |
| primitive-string-method | 1.03–1.03× | 2.28–2.36× |
| proto-method | 1.02–1.03× | 2.36–2.39× |

All 33 before/after pairs remain within the noise threshold; no throughput
speedup or regression beyond that threshold is established on this runner.
All 33 Boa pairs still find Temiya slower beyond the threshold. Both all-eleven
speed gates remain false. Unary/update and Reference integration does not satisfy the Boa goal
or complete #307/#427.
