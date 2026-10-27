# Realm native function prototypes (#467)

Builtin registration and Array/String method registration now share native
callable allocation with the owning Realm's Function.prototype, configurable
name/length and ordinary method property attributes. Methods stay callable
without acquiring [[Construct]] or an own prototype property. Function.prototype
itself still inherits Object.prototype. The Proxy constructor's prototype link
is also corrected; this does not complete its other constructor semantics.

`Realm::create_native_function(name, length, callback)` provides the same contract
to hosts. Engine::register_global_function uses length zero because that API
takes no declared arity. Low-level Object::native_function remains independent
of any Realm; raw host-created objects are not automatically adopted. The factory
stores the callback directly and captures no Realm or registry. Fresh prototype
initialization does not traverse user Proxy chains after Function.prototype's
prototype is mutated.

AST functions inherit Function.prototype. Bound functions retain the target's
actual prototype and use a weak constructor self slot, preserving newTarget
handling without the old self-retention cycle. Their full name/length behavior
remains #210. Generator bridge methods, async Promise methods/derived Promises/
thenable resolving functions, Proxy revokers and module namespace getters receive
the owning Realm's prototype. Low-level queue-only Promises remain Realm-free.

Original propertyHelper.js also needs own-property query APIs. This change exposes
Object.getOwnPropertyNames/getOwnPropertySymbols with ordered fallible OwnKeys and
actual Array results. Object.prototype.propertyIsEnumerable and hasOwnProperty
convert the key with the string hint before checking/boxing the receiver, then
use fallible own-descriptor lookup. Proxy exceptions and original thrown identity
propagate. Primitive wrapper identity cannot be observed by these own-property
queries; only strings contribute own primitive-wrapper properties.

Public Engine/host tests cover actual call/apply/bind invocation, metadata,
constructability, two-Realm identities, native captures after Realm handle drop,
bound native target release, callback order, Proxy errors and fresh allocation.
Intrinsic constructor/prototype graphs already contain strong cycles. The added
standard prototype edges extend those graphs; complete reclamation still requires
runtime tracing GC (#313). These lifetime tests establish release of isolated
host callback captures and the removed bound self cycle, not full Realm teardown.

The fixed original upstream harness and whole test programs run unchanged. Their
remaining failures stay separate from normal tests and are not counted as skips.
Shared function creation and builtin metadata affect setup and allocations, so
this source requires its own comparative measurements. No Boa speedup is claimed.

## Earlier upstream snapshot (runtime 1090296)

On stable and Rust 1.85.0, the unchanged 42-file/75-variant global selection
advances from 53 pass / 22 fail to **61 / 14**. All eight original propertyHelper
blocked global variants now pass. The unchanged numeric selection (31 files/62
variants) advances from 40 pass / 22 fail to **58 / 4**. No previous pass regresses
and no cases are skipped. Full selections still exit 101; the targeted eight-case
gate exits successfully. This is cumulative integration evidence with explicitly
recorded baseline revisions, not an isolated attribution to one added API.

[native-prototype-audit.json](../test262/native-prototype-audit.json) retains all
outcomes, fixture identities and status transitions for both toolchains. Remaining
global and numeric failures remain recorded. Normal stable/MSRV all-target tests
each pass **374 tests**, with 9 ignored; formatting and all-target/all-feature
Clippy with warnings denied pass. That snapshot predates parent integration; current results are recorded below.


## Main integration and current validation

The source integrates merged main `190b01f5de6ae323e0149ea4c164bdfa5b4f37b6`.
Native method factory changes preserve the main branch’s bound-target and weak
constructor-self behavior. Own-key queries return actual Array exotics: public
Array.isArray and length-truncation behavior are tested, as well as their Realm
Array.prototype. Registered host callback captures are released after their Realm
handle is dropped; retained standalone callbacks still work after that drop.
This remains separate from full intrinsic graph reclamation.

Current stable/MSRV all-target suites each pass **438 tests**, with 12 ignored;
final focused tests pass nine on both toolchains. Formatting and all-target/
all-feature Clippy with warnings denied pass. The original numeric selection
remains **58 pass / 4 fail**, the global selection **61 / 14**, and the binary
selection **31 AST pass / 0 fail**, **21 VM pass / 0 fail / 10 unsupported**. All
eight originally blocked global variants still pass. Full selections still exit
101; no previous pass regresses and no case is skipped.

[native-prototype-main-audit.json](../test262/native-prototype-main-audit.json)
retains the post-integration reports and their exact source/baseline identities.
Numeric a-z now reaches the missing String.fromCharCode API after the parent
BigInt/Number comparison fix; Symbol-constructor prerequisites also remain. These
failures are retained. This source needs its own hosted CI/security and original
eleven-workload comparison; the older reports do not establish its throughput.


## Hosted x86_64 native function prototype comparison

[Run 40431339090](https://github.com/ieee0824/temiya/actions/runs/40431339090)
completed on Ubuntu 24.04, AMD EPYC 9V45 96-Core Processor, four logical CPUs. The
[provenance record](../benchmarks/native-prototypes-hosted-provenance.json) retains
build identity and the original artifact reference. Both
[before/after](../benchmarks/native-prototypes-hosted-beforeafter.json) and
[Boa](../benchmarks/native-prototypes-hosted-boa.json) use the original eleven
workloads, 15 timed samples, one checked warmup and three alternating pairs.
Each mode has 66 successful runs and 1056 validated values. They run serially
after all builds, with the same compiler and existing 512 MiB/180-second guards.

The measured revision is `28d23cd276528df755955d5bb3392566cea89cb5`;
the baseline is `190b01f5de6ae323e0149ea4c164bdfa5b4f37b6`. The measured
runtime is identical to code revision `28d23cd276528df755955d5bb3392566cea89cb5`.
Later result-only commits do not modify executable source, worker scripts,
dependencies or the measurement workflow. Architectures are reported separately.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 0.95–1.00× | 3.23–4.42× |
| prop-mono | 0.97–0.99× | 2.37–2.46× |
| prop-mega | 0.97–0.98× | 2.21–2.23× |
| call | 1.02–1.03× | 2.32–2.94× |
| closure-alloc | 1.00–1.01× | 1.86–1.95× |
| object-alloc | 1.00–1.01× | 1.76–2.11× |
| string-concat | 0.97–0.99× | 2.85–3.56× |
| array | 0.98–1.00× | 2.05–2.26× |
| primitive-string-property | 0.95–1.00× | 2.61–3.20× |
| primitive-string-method | 0.96–0.99× | 2.02–2.41× |
| proto-method | 0.97–1.01× | 2.19–2.61× |

All 33 before/after pairs remain within the noise threshold; no throughput
speedup or regression beyond that threshold is established on this runner.
All 33 Boa pairs still find Temiya slower beyond the threshold. Both all-eleven
speed gates remain false. Native function prototype integration does not satisfy the Boa goal
or complete #307/#427.

### Startup is a separate observation

The same report records one startup timing per worker process, outside the
repeated invocation boundary. Across 33 observations each, startup medians are
123,767 ns before and 160,662 ns after (about +29.8%, +36,895 ns). This is an
observed startup cost; unchanged invocation classification does not establish
unchanged Realm initialization time. The raw values remain retained, and reducing
metadata/allocation cost without removing standard behavior is tracked in #478.
