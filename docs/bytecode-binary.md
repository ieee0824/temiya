# Binary and compound operators (in progress, #287)

The explicit VM lowers arithmetic, remainder, exponentiation, bitwise and shift
operators, strict/loose equality, relational comparisons, `in`, `instanceof`,
short-circuit `&&`/`||`/`??`, and all corresponding compound assignments.
There is no AST fallback for an unsupported test program. Engine still uses AST.

Compound assignment retains one Reference before reading its value and evaluating
the RHS. Computed object keys convert once; writes retain the original receiver.
Logical assignments skip both RHS and setter when the old value determines the
result. The verifier checks retained References and operand stacks across joins.
The existing numeric JIT opcodes remain; generic operators require the VM.

AST and VM share Realm-aware object conversion. Numeric operators convert the
left operand to a numeric value before converting the right. Relational
operators first convert both primitives in source order. Addition uses the
default hint, and `in` validates its RHS before converting its key with the string
hint. Loose equality with null/undefined does not convert objects. BigInt/Number
comparisons use the exact IEEE-754 value, including fractions and infinities.
Huge positive BigInt right shifts saturate without allocating their shift count;
constant BigInt powers and Number exponentiation edge cases are covered.

`instanceof` supports custom `Symbol.hasInstance`, the intrinsic
`Function.prototype[Symbol.hasInstance]`, bound targets and fallible Proxy
prototype traversal. Primitive LHS values return before reading the ordinary
constructor's prototype. User-defined AST functions now inherit Function.prototype
as VM functions already do. Bound functions inherit the target's prototype and
use a weak self slot; a lifetime test verifies release of their captured native
target. Fresh function/instance prototype initialization does not traverse user
Proxy chains. Native builtin prototype coverage (#467), full bound function
metadata and cross-Realm construction remain separate unfinished work.

The normal tests exercise whole programs in both AST and VM, real getters and
setters, conversion order and abrupt completions, Symbol/BigInt boundaries,
logical assignments, bound target dispatch and Proxy traps. The pinned original
Test262 selection is an additional opt-in gate; failures and compile-unsupported
programs are retained, never counted as pass or skip. BigInt wrapper coercion
(#226), object literal lowering and complete upstream operator coverage remain
unfinished. This does not close #287 or #176.

Shared hot paths, function prototype setup and Object layout change. Previous
unary/operator measurements do not establish this revision's performance; fresh
before/after and actual omoikane Boa comparisons are required. No performance
improvement or Boa superiority is claimed.

## Fixed original Test262 audit

The 16-file selection has 31 strict/sloppy variants. With identical original
fixtures and host harness, VM results advance from 0 pass / 0 fail / 31
compile-unsupported to **19 pass / 2 fail / 10 unsupported**. AST advances from
15 pass / 16 fail to **25 pass / 6 fail**. Stable and Rust 1.85.0 agree, with no
previous pass regressed. All four opt-in runs exit 101; the gate remains false.
[binary-audit.json](../test262/binary-audit.json) retains fixture blob IDs, exact
runtime revisions, all outcomes and transitions. The baseline runtime is unchanged
and receives only the copied audit test file.

The remaining executed failures involve BigInt wrapper conversion; VM compile
unsupported includes object literal expressions and other prerequisites in the
original programs. These results do not establish complete operator support.
Normal all-target tests on both toolchains pass **424 tests**, with 11 ignored.
Formatting, Clippy on all targets/features with warnings denied, and public
backend examples on both toolchains pass on ARM64. The JIT reports its expected
unsupported platform result here; x86 execution is checked separately by CI.


## Hosted x86_64 binary/compound operator comparison

[Run 40421868896](https://github.com/ieee0824/temiya/actions/runs/40421868896)
completed on Ubuntu 24.04, AMD EPYC 7763 64-Core Processor, four logical CPUs. The
[provenance record](../benchmarks/bytecode-binary-hosted-provenance.json) retains
build identity and the original artifact reference. Both
[before/after](../benchmarks/bytecode-binary-hosted-beforeafter.json) and
[Boa](../benchmarks/bytecode-binary-hosted-boa.json) use the original eleven
workloads, 15 timed samples, one checked warmup and three alternating pairs.
Each mode has 66 successful runs and 1056 validated values. They run serially
after all builds, with the same compiler and existing 512 MiB/180-second guards.

The measured revision is `ca675c76ad3cafdcacb79bb4a70e03f9ab3a904b`;
the baseline is `c84c88fb456c9b485a4c9350410efe29878968af`. The measured
runtime is identical to code revision `ca675c76ad3cafdcacb79bb4a70e03f9ab3a904b`.
Later result-only commits do not modify executable source, worker scripts,
dependencies or the measurement workflow. Architectures are reported separately.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 0.95–0.97× | 3.23–3.42× |
| prop-mono | 1.01–1.02× | 2.17–2.55× |
| prop-mega | 1.01–1.01× | 2.30–2.35× |
| call | 0.97–0.98× | 2.36–2.49× |
| closure-alloc | 0.98–1.01× | 1.74–1.78× |
| object-alloc | 0.99–1.01× | 1.67–1.70× |
| string-concat | 0.93–0.95× | 2.93–3.13× |
| array | 1.00–1.04× | 2.28–2.30× |
| primitive-string-property | 0.96–0.98× | 2.65–2.70× |
| primitive-string-method | 1.00–1.02× | 1.92–1.96× |
| proto-method | 0.99–1.00× | 2.22–2.33× |

All 33 before/after pairs remain within the noise threshold; no throughput
speedup or regression beyond that threshold is established on this runner.
All 33 Boa pairs still find Temiya slower beyond the threshold. Both all-eleven
speed gates remain false. Binary/compound operator integration does not satisfy the Boa goal
or complete #307/#427.
