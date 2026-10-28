# Runtime ownership migration

Decision for #256, based on main `4866b4de8ea03de01066fcfd450b9a16d011c948`.
This is the implementation contract for #257, #313 and #340. The public Engine
still uses Rc objects. The collector in `gc.rs` is a separate component; this
plan and its probes do not make runtime collection implemented.

## Evidence and acceptance probe

The fixed omoikane closure-allocation workload creates 300,000 ordinary
functions and expects 715003. All three runs in
[`string-core-comparison.json`](../benchmarks/string-core-comparison.json)
exceed the 512 MiB watchdog. Changing that limit or reducing the iteration count
cannot satisfy the original workload.

`examples/ownership_probe.rs` uses the public Engine to execute seven small JS
fixtures. A native callback captures one Rc token, observed only through a Rust
Weak pointer. Each returned value is cloned; the script removes the callback's
global binding and clears the JS roots. Compiled scripts, Realm, and Engine are
then dropped. Both surviving value handles must keep the callback usable.
Only after both handles and transient callback values are dropped is token
release required. An escaped prototype must also retain a callable constructor,
and a captured lexical callback must remain callable.

```sh
cargo run --locked --example ownership_probe
cargo run --locked --example ownership_probe -- --require-reclamation
```

The diagnostic invocation emits JSON and exits zero after safety checks. The
strict invocation emits the same observations and exits **2** while any token
survives teardown. The recorded
[baseline](../benchmarks/runtime-ownership-baseline.json) released the acyclic
control but retained all six cyclic/function cases. See the function-allocation
update below for current observations. This is an observed failure,
not an ignored test or a claimed collector pass. `tests/runtime_ownership.rs`
checks live-handle/clone/function/prototype safety and acyclic release; it does
not require known leaks to persist after migration. The strict command is the
post-migration reclamation acceptance test.

These probes cover runtime teardown, not collection within a live Engine.
After integration, add the explicit `Engine::collect_garbage` development hook
and run the same root transitions before Engine teardown, collecting after each
step. That API does not exist today. Pending jobs, suspended generators,
modules and ephemerons have additional phase gates below; the seven fixtures
alone cannot establish complete GC integration. No Test262 pass count is
assigned to host-token destruction: the standard does not expose this Rust
ownership observation.

## Complete current Engine edge inventory

An edge means a value-bearing field, including values inside containers and
opaque native captures. Scalar IDs, names, booleans, source positions and AST
syntax have no runtime object edge. S means strong reachability, W weak,
E ephemeron, R externally registered root. All rows must be ported before
runtime allocation collection is enabled.

| Owner / source | Current outgoing fields | Migration rule |
| --- | --- | --- |
| `Value`, `RuntimeError` (`runtime.rs`) | Object; error `thrown: Box<Value>` | S; trace thrown payloads on every error path |
| `SymbolData`, `JsString` | Symbol description is JsString; strings own UTF-16; BigInt owns numeric storage | Leaf owned Rust payloads for phase one; preserve symbol identity; no heap object edge |
| `PropertyDescriptor` (`object.rs`) | `value`, `get`, `set` | S for present values/accessors; attributes are scalars |
| `ObjectData`, `ObjectState` | prototype; named property keys/descriptors; elements; primitive_data; callable; construct; conversion.exotic/value_of/to_string | S for prototype/descriptors and primitive object payload; native captures need explicit tracing, not opaque closure inspection |
| `Elements` (`elements.rs`) | Packed descriptors; present Holey descriptors; Sparse descriptors | S for descriptor edges in all representations; holes/indices/counts are leaves |
| Array/String/Number/BigInt/Date wrappers | Wrapped Object; String string_data; primitive_data; date_value | S wrapper object; string/numeric/date payloads remain leaves |
| BufferSlot / ViewSlot (`indexed.rs`) | Buffer Rc byte vector; view buffer, offset, length | S to shared byte backing store, not to an invented ArrayBuffer object; backing data remains alive through a view after wrapper loss; detach invalidates all views |
| RegExpSlot (`regexp.rs`) | source, flags, matcher, last_index | Leaf owned Rust data; wrapper Object is S |
| MapSlot / SetSlot (`collections.rs`) | Map key/value entries; Set values | S, including keys and every nondeleted entry |
| WeakMapSlot / WeakSetSlot | WeakMap WeakObject keys and strong Value values; WeakSet WeakObject keys | E key→value for a live WeakMap; W for keys/WeakSet; value→key must not make its key live by itself |
| ProxySlot (`proxy.rs`) | optional target/handler pair; call/construct closures capture slot | S target/handler until revoked; clear both on revoke; migrate callable payload together with slot |
| EnvironmentData (`environment.rs`) | outer, record | S outer and record edges |
| DeclarativeRecord / Binding | bindings[].value | S; TDZ `None` is no edge; mutable/strict/deletable flags are leaves |
| ObjectRecord / GlobalRecord | binding_object; declarative bindings; global_this_value | S; options and var names are leaves |
| FunctionRecord / ModuleRecord | bindings; this_value; function_object; new_target; imports[].target_environment | S including imported live-binding environments; names/status are leaves |
| RealmData / IntrinsicRegistry (`execution.rs`, `intrinsics.rs`) | global object/environment; queue; intrinsic object map; well-known Symbol map; HostHooks | S intrinsic/global/queue; symbols leaf-owned; host clock callback is opaque external state |
| FunctionClosure (`function.rs`) | realm, environment, lexical_this, home_object; code | S runtime fields; AST immutable owned data; current call/construct callbacks also capture self slot |
| Function self slot / default prototype | Rc slot contains function; function.prototype.constructor contains function | Self-identity lookup is non-owning; visible prototype/constructor edges remain S. Replace hidden callback ownership with typed FunctionData |
| ObjectConversion / native builtins | closure captures (including Coercion's five wrapper prototypes and constructor prototype captures) | Typed traceable capture records for builtins; an empty JS program still has intrinsic cycles |
| Bound functions (`builtins.rs`) | target function, bound receiver, prefixed arguments; construct closure target/prefix | S all captures; ordinary callback closure erasure must not hide them |
| JobQueueData (`job_queue.rs`) | jobs FnOnce captures; unhandled_hook captures | S managed job payloads; executing popped job becomes an active root before a safepoint; host hook state follows native callback contract |
| Promise / PromiseData (`promise.rs`) | object; data; queue; fulfillment Value/rejection RuntimeError; reaction callbacks and result Promise | S including thrown rejection values and result capability; pending is no settlement edge |
| Promise thread-local RegistryEntry | **strong object and queue**, weak data, identity | Remove strong lookup roots; put PromiseData on managed object and use W lookup indices if still needed. Existing registry is not wholly weak |
| GeneratorData (`generator.rs`) | evaluator, statements, continuation, delegated values, returned Value | S suspension state; Continuation::AssignReference traces Reference; other continuation expressions/body are AST leaves |
| ExpressionEvaluator / StatementEvaluator | realm, environment, this_value, new_target; variable_environment | S when live/suspended; strict flag leaf |
| Reference (`interpreter.rs`) | base Environment/Property Value/Unresolvable global Object; computed name Value; this_value; primitive_prototype | S all variants and optional fields; property-name Symbol/String is leaf-owned |
| Completion / Resume / IteratorResult | normal/break/continue/return/throw Value; resume input; yielded value | S even on abrupt completion, finally override and suspended return |
| ScriptData (`execution.rs`) | realm, program | S realm, owned AST leaf; retained compiled script is an external root |
| ModuleData / Dependency (`pipeline.rs`) | realm, environment, dependency modules, cached error, evaluation Promise | S; cycles are traced, not individually pinned; status/specifier/program are leaves |
| ModuleGraph | realm, resolver callback, cached modules | S while graph retained; resolver capture contract matches host callback; ModuleKey/request strings/attributes are leaves |
| ExecutionContext / Agent | realm, script, function, lexical/variable environments; retained stack entries | Active R regions; suspended/protected entries retain their full graph until removed |
| Engine / EngineBuilder (`embedding.rs`) | currently options, Box<dyn Any> host_state; no Heap | Engine owns shared Runtime in target design; opaque host state can own explicit external roots |
| RealmHandle / CompiledScript / ModuleHandle / ValueHandle / Exception | Realm / ScriptRecord / SourceTextModule / Value / thrown ValueHandle | R registered persistently with origin Runtime and existing provenance checks |
| HostCallContext / host arguments and results | IDs only in context; arguments, receiver and return handles have Values | Active R for call; validate result provenance before adopting its internal edge |
| FFI wrappers / registry (`ffi.rs`) | boxed Engine/Realm/Script/Value handles; registry stores kind/address only | Wrappers own R, release exactly once; address registry is not an extra GC root; CString errors have no Value edge |

The separate bytecode/JIT/debugger/collector development components are not
selected by Engine today. Their register/frame/cache handles require an
additional tracing audit before connection (#307, #324, #340). They cannot
supply missing roots for the current AST path.

## Selected ownership representation

Use the existing nonmoving, stop-the-world mark/sweep Heap first. Each Engine
owns `Rc<Runtime>` with that heap, persistent root registrations and active frame
regions. Public handles retain Runtime ownership and registered roots, so a
handle can outlive the Engine wrapper. Managed allocations never own
`Rc<Runtime>` or `Root<T>` back to their owner: that would reproduce a root cycle.
Internal fields store non-owning `Handle<T>` and private `StoredValue` variants;
public `Value::Object`/Object facades own rooted references. Boundary conversion
between stored and rooted values is explicit. Ordinary JS evaluation uses the
internal representation and does not register every expression result in the
persistent root HashMap.

Object, Environment, Realm, FunctionData, PromiseData, generator state, module
records and managed job payloads implement Trace. Leaf backing bytes/AST/Regex
can remain ordinary Rust allocations owned by these records. No raw address is
exposed; IDs and handles retain heap/slot/generation checks. Preserve exact
Object identity, descriptor attributes, enumeration order and Realm semantics.
Do not weaken prototype.constructor, outer environments, or live callbacks.
The function self-lookup is implementation metadata and can be non-owning;
that alone does not reclaim ordinary function/prototype or environment cycles.

Low-level allocation becomes explicit in its Runtime context. Migrate internal
constructors to `new_in(runtime, ...)`; do not allocate one hidden heap per
object and do not silently copy/adopt foreign cyclic graphs. Removing existing
context-free runtime constructors or changing public Value internals requires
the documented pre-1.0 minor release (0.2) and a migration guide. Keep the C ABI
handle ownership contract; extend it only with a versioned collection API.
Engine::value must verify object heap ownership as well as Engine/Realm IDs.
Plain runtime Values extracted with into_value remain rooted facades; this
escape hatch cannot turn a live public value into a dangling internal handle.

Typed JS/builtin callable records replace Fn captures of managed state. Store
managed captures as explicit StoredValue/Handle fields and trace them. Host
callbacks remain capable of owning external roots, just as any host may retain
a ValueHandle. Rust closures/Any cannot be introspected. Provide a declared
managed-capture API for callbacks needing collectible JS cycles; ordinary
opaque host state must release retained roots explicitly. The collector must
never assume such a capture is weak or invoke arbitrary JS/host code to trace
it. This limitation must be part of the embedding contract and leak diagnostics.
Every internal builtin/job must use the traceable path, not opaque external
roots. Engine destruction releases host state before dropping its Runtime root;
remaining public handles retain Runtime independently. A host-created Rust
cycle retaining Runtime remains the host's explicit ownership responsibility.

## Collection and rooting protocol

1. Root every incoming receiver/argument, newly produced object, pending error
   payload and incomplete expression result in a scoped frame region before
   an operation that can allocate or invoke JS. This includes a binary left
   operand, assignment Reference, callee, arguments already evaluated, and
   partially constructed literals/descriptors.
2. Copy required fields/handles out of a heap/object borrow, then release the
   borrow before a getter, Proxy trap, host callback, nested allocation or job.
   Never collect with an outstanding Heap::with_mut/RefCell borrow. Handle
   identity may cross a safepoint; an interior Rust reference may not.
3. Initial safepoints are explicit allocation/loop-backedge/call-return sites
   with registered frames. Publish/root a new allocation before another
   collection opportunity. Existing Heap automatic pre-allocation collection
   must be invoked only after this protocol protects all inputs. Stress mode
   collects at every eligible allocation to expose missing temporary roots.
4. Mark persistent roots and active regions, then trace strong edges. Process
   live WeakMap ephemerons to a fixpoint before clearing weak handles/sweeping.
   Remove dead registry/cache entries. Heap destruction reclaims all managed
   records when the final external Runtime owner is dropped, including cycles.
5. Sweep/destruct without executing JS. Queue user cleanup for a later explicit
   checkpoint, rooted while queued. A popped job is protected before invoking
   callbacks. Catch host panics at the existing embedding boundary; RAII must
   unregister frame regions on normal return, throw and panic unwind.

Arbitrary Rust host state may have a reentrant destructor. Detach retired host
payloads during sweep and run their Rust destructors only after heap borrows are
released and the heap is consistent, under a host panic boundary. Host reentry
then creates a fresh rooted frame. Defer recursive collection during collection;
final Runtime teardown rejects new entries once closing starts. Dropping the
Engine wrapper does not start closing while public handles still own Runtime.

Keep existing full-collector weak/ephemeron verification as the oracle. Do not
enable generational, incremental, moving or concurrent runtime collection in
this first integration. Existing barriers/collect_minor are isolated components;
wire and verify every old→young write before enabling a generational mode.

## Delivery gates and dependencies

| Stage | Work / owner issues | Required executable evidence |
| --- | --- | --- |
| 0 | This ownership decision (#256) | Public probe observations, strict teardown gate, live-root tests, current-source edge inventory |
| 1 | Context allocation, StoredValue, traced Object and descriptors (#257), typed callable/Environment/Realm payloads (#313), boundary roots (#340) coordinated | Same-heap validation; stale/cross-heap handles fail safely; all seven probe cases release after final root; live clones/closures/escaped prototype still work |
| 2 | Active evaluator regions and safe allocations (#313, #324, #340) | Collect during every eligible allocation, including getters/Proxy/host reentry, binary/assignment temporaries, failed constructors, finally and arbitrary throw payloads |
| 3 | Job/Promise/module/generator tracing and weak collections (children of #133) | Queue-only callback survives until run; popped job survives nested collection; pending/suspended captures and cyclic module imports survive live roots and release after removal; WeakMap value→key cycle does not pin a dead key; no strong Promise registry roots |
| 4 | Runtime collection enablement and performance | Teardown gate and live-Engine forced collection both pass; all normal tests and pinned suites; original closure workload at 300000 iterations / expected715003 under unchanged512MiB; all11 repeated Boa comparisons with correct results |

Stages 1–3 are one coordinated graph migration: partial components may be
reviewed separately, but runtime allocation collection stays disabled until all
reachable edge families and active roots are covered. A failure in the strict
probe blocks implementation completion, not the recording of this design.
Add per-family drop/weak probes as each managed payload is introduced; the
original seven are the minimum gate, not the full graph test suite.

For speed, record allocation/root-registration counts, peak live managed cells,
collection time and mutator throughput separately. Keep AST data immutable and
shared where practical; avoid per-call AST copies and per-expression persistent
root allocation. Lazy default function prototypes are a possible independent
allocation optimization, but must preserve data descriptors, own keys,
constructor identity and escaped prototypes. Replacing them with accessors or
weak constructor edges changes observable semantics. Neither lazy creation nor
removing the hidden self slot constitutes a collecting runtime.

Reject two alternatives: keeping Root values inside heap records permanently
roots cycles; applying Rc trial deletion without enumerating native captures
cannot distinguish hidden internal edges from external references. Both still
need the explicit graph and callback work selected above.

## Function allocation retention update

The function-allocation change keeps the internal callable self lookup weak.
For constructable ordinary functions, the own `prototype` data property is
registered at creation with its standard writable/nonenumerable/nonconfigurable
attributes and insertion position. Its initial object value is materialized
when GetOwnProperty (including Get) or DefineOwnProperty first observes it.
OwnKeys and rejected deletion do not require the value. A materialized prototype
inherits Object.prototype and has the standard nonenumerable `constructor`
backlink. PreventExtensions does not prevent the logical existing property's
value from being materialized; redefinition still validates the actual value.

The edge inventory gains one strong pending Object.prototype reference in
ObjectState. The self lookup becomes W; once materialized, the visible
function/prototype/constructor graph remains strong. Descriptor observation and
escaped prototypes intentionally preserve that graph. This update does not
enable collection and does not trace arbitrary object/environment cycles.

The original seven-fixture probe now releases arrow, ordinary anonymous and
lexical-closure cases, as well as the acyclic control. Three cases still retain
values: self-cycle, two-object-cycle and escaped-prototype. The historical
baseline above is unchanged. Current observations are recorded separately in
[function ownership observations](../benchmarks/function-ownership-observations.json).
The strict reclamation gate remains failing until those cycles are collected.

`tests/function_allocation.rs` covers data descriptors, order, replacement,
nonwritability/nonextensibility, Proxy invariants, escaped/live constructors,
removal of active call targets, and release after an unobserved prototype or
OwnKeys. Its ignored upstream selection executes four original Test262 files
(`language/statements/function/S13.2_A1_T1.js`, `S13.2_A1_T2.js`,
`S13.2_A4_T1.js`, `S13.2_A4_T2.js`) in strict/sloppy modes at the pinned revision:

```sh
TEST262_DIR=target/test262 cargo test --locked --test function_allocation \
  pinned_test262_function_prototypes -- --ignored --nocapture
```

This is eight passing variants, not full Function conformance. Function name
inference, restricted properties, broader metadata and full prototype-chain
coverage remain tracked separately (#202, #210).

## Pending 0.2 API migration

The coordinated tracing migration changes `Realm::intrinsics()` from a borrowed
registry to an owning `IntrinsicRegistry` snapshot. The snapshot and its clones
keep their intrinsic objects alive after the original Realm facade is dropped.
Native functions retain their source Realm through a strong traced edge, so a
snapshot containing those functions also keeps the Realm graph alive. The
Realm's internal table stores traced handles and does not register those objects
as external roots.

Bind the returned registry before retaining an iterator or passing a reference
to another API:

```rust,ignore
let registry = realm.intrinsics();
let entries = registry.iter();
let ids: Vec<_> = entries.map(|(id, _)| id).collect();
```

Use `realm.intrinsic(id)` when only one object is needed; this avoids constructing
a snapshot of the complete table. Object identities remain stable across
snapshots. This change is part of the required 0.2 release, not a compatible
0.1 patch. The tracing branch must still satisfy the coordinated graph,
reclamation, rooting, ABI, and performance gates above before release.

Collection writes must also share their owner's allocation context. Use
`MapObject::try_set` and `SetObject::try_add` to receive a `RuntimeError` for a
foreign Object key/value before any entry is changed. The existing `set`/`add`
conveniences panic on this error; JavaScript builtins use the fallible methods.
Primitive keys and values remain immediate stored values. Public Map/Set
iterators retain their owning Object and its runtime graph until they are
dropped, including when the original collection facade has been released.

Managed WeakMap entries store weak key identities and non-owning stored values.
Their values are reported with `Tracer::ephemeron` only while the map itself is
traced. Full collection requires both the owner and key to be reachable before
tracing an object value; a value-to-key cycle cannot make its own key live.
Minor collection conservatively retains old allocations, processes conditional
edges from rooted/remembered owners, and applies the same fixed point to young
keys and values. Insertion records barriers for both the key and object value.
Dead entry cleanup checks generation-valid identities without creating key
facades or root registrations. WeakSet stores only weak identities and reports
no strong key edges. The standalone `Heap::add_ephemeron` API remains a global
key/value association and is not used for managed WeakMap ownership.

Managed Realm construction allocates the Realm and its Global Environment
before building intrinsics. Each startup callable receives a source Realm edge;
later native/typed builtins inherit it from Function.prototype's internal
metadata, independently of mutable JavaScript prototype links. A complete
stored intrinsic table is published before installing names on a host global
Proxy. Its DefineProperty traps can therefore inspect the source Realm and run
collection without seeing an incomplete table. Publication records normal
Realm-to-object write barriers.

Native callback payloads hold this metadata inside their existing Rc allocation;
call/construct trait objects keep the same pointer width and ordinary Object
records gain no Realm field. Managed callable payloads use the same header;
AST/VM closures keep their existing explicit Realm edge without duplicating it.
Successful calls do not load or root a Realm. Unmaterialized native exceptions
load the source Realm only on failure, before crossing a caller boundary.
Escaped native functions and exceptions keep the source graph alive, and its
cycles are reclaimed after the last external root is dropped. Transitional
Legacy objects do not acquire native source metadata; removal of that
representation and enabling managed allocation remain required release gates.

Realm job queues now have managed heap records and rooted public facades. The
Realm stores a typed queue handle rather than an owning public queue. Repeated
public queue loads/clones share one root registration; an escaped empty queue
can outlive its Realm without retaining that Realm's intrinsic graph.

Internal `TracedJob` payloads report their stored edges. Enqueue validates those
edges against the queue's heap and records its write barrier before publication.
Dequeue moves a job to a traced active list before invoking it, so collection in
callbacks or nested job runs still sees every capture. Scoped guards remove
active entries and reset draining state on success, errors, and unwinding. Jobs
are invoked after releasing the queue's state borrow. Public host `enqueue`
wraps the host FnOnce in one Rc allocation and retains the explicit host-root
contract; internal Promise jobs still require conversion to typed payloads.

Promise objects now own their internal state through an Object slot. Identity
lookup reads that slot directly; there is no thread-local table retaining
Promise objects or queues. Managed state stores fulfillment values, thrown or
materialized error payloads, reaction callbacks/results, Function.prototype,
and the queue through typed non-owning edges. Public Promise facades root their
Object and queue; internal reaction records retain no public Promise facade.

Promise then/resolve/reject functions use declared target captures. Reaction,
thenable, and unhandled-rejection jobs implement TracedJob, so dequeued jobs keep
all pending values reachable through the queue's active list. Pending reactions
stay attached to the Promise while the observable state setter runs, before
being published as jobs. Rejection payload identity is preserved across await,
reaction calls, and host hooks. Cross-context writes are rejected before state
or reaction-list mutation. These changes migrate the existing Promise/async
implementation; they do not add Promise constructor completeness or bytecode
support for async syntax.

Generator objects now own a traced slot with a stored suspended frame. The
frame stores evaluator Realms/environments/this/newTarget, statement variable
environments, pending Reference bases/names/receivers/primitive prototypes,
delegated values, and pending return values through non-owning edges. Computed
Reference names keep their unresolved value and @@toPrimitive identity without
coercing at suspension. Mutable function-binding position hints remain hints
and are validated by the existing Reference algorithms when restored.

Resumption takes the stored frame out of the slot and marks the generator
Executing before invoking continuations or user code. Active evaluator and
Reference facades root the frame on the Rust stack; no generator RefCell borrow
spans JavaScript callbacks. A scoped guard completes and releases the frame on
errors/unwinding. Successful suspension records write barriers and publishes the
stored frame without an intervening managed allocation or callback. Completion,
throw, and return clear suspended captures while escaped error values retain
their own roots. Generator methods use declared strong target captures and
iterator-result objects allocate in their source context with Object.prototype.
Code vectors move between active/suspended states without per-resume deep
copies; yield* resumes pop one stored delegated value without decoding or
rewriting the remaining vector. Existing generator syntax/protocol limitations
and the Legacy representation remain pending release gates.

Source Text Module records now use the shared managed heap with rooted public
facades. Their dependency records, Realm/environment, cached evaluation Promise,
and cached exception payloads are typed stored edges. Public ModuleGraph caches
retain module roots while the graph is retained; dependency cycles contain no
public module/Realm/Promise roots. Facade clones and reloads share one root.

Namespace objects are cached per module, have a strong stored owner edge even
when there are no exports/getters, and retain typed live-binding getters rather
than opaque closures owning module facades. Escaped namespaces therefore keep
cyclic dependency graphs and live bindings valid after ModuleGraph is dropped.
Cached Promise/error values can separately outlive their module records. Module
evaluation creates Promise functions with the known source Realm metadata.
Parsing moves the Program into its record after collecting requests, avoiding
the prior full AST clone. Existing linking/evaluation and module-namespace
conformance limitations remain separate from this ownership migration.

Primitive boxing now shares one context-aware wrapper allocator between the
Object constructor, AST/VM sloppy-this binding, and with statements. It selects
only the required intrinsic prototype and initializes actual String/Number/
BigInt or Boolean/Symbol wrapper slots; AST this binding no longer exposes a
synthetic [[PrimitiveValue]] property. Null/undefined sloppy-this binding uses
the Realm's chosen global-this value, matching the VM and host Realm contract.
With still rejects null/undefined before boxing.

Read-only own-key/own-enumerability queries on primitives do not allocate
wrapper Objects. Strings report UTF-16 indices and nonenumerable length; other
non-null primitives have no own properties. Object queries retain their fallible
Proxy operations. Property-key conversion still precedes null receiver errors.
This removes the remaining contextless temporary boxing in these execution
paths, without adding a root registration per primitive property check.

## Allocation safepoint audit on the tracing branch

All five managed allocation families (Object, Environment, Realm, JobQueue,
SourceTextModule) now collect before allocation through a Runtime safepoint.
The configured production interval remains disabled during migration. Tests set
an interval of zero to collect at every allocation, including Realm/intrinsic
bootstrap, AST and VM closures/constructors, builtin result creation, generators,
Promise reactions, and cyclic module loading/evaluation.

The incoming unpublished record is an additional scoped tracing root during
collection. Heap automatic threshold/capacity collection likewise traces its
incoming value; the regression previously reclaimed a target referenced only by
the new allocation. Retirement completes under the Heap borrow, then payloads
are dropped after releasing that borrow. Reentrant allocation during host Drop
skips nested collection until retirement finishes, and stale handles remain
invalid after slot reuse. Scalar values do not gain persistent root slots.

The original seven teardown fixtures are reused unchanged through an injected
Engine constructor. They now pass strict reclamation with managed allocation
and every-allocation GC, without an extra collection after final handle drop.
An additional test runs the same root/clone transitions with a live Engine and
collects after each transition, preserving callbacks/constructors/closures and
releasing the host token after the last handle. These are internal migration
checks; the public default, strict CLI default, API/version migration, complete
stress/ABI/toolchain gates, and performance qualification remain pending.
