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

## Original upstream evidence

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
Clippy with warnings denied pass. Hosted CI/security and source-specific speed
measurements remain required. Parent integration and its revalidation come next.
