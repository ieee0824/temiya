# Runtime ownership migration

Decision for #256, based on main `4866b4de8ea03de01066fcfd450b9a16d011c948`.
This is the implementation contract for #257, #313 and #340. The migration
branch now uses a shared tracing heap for public Engine Realms, with allocation
safepoints and an explicit `Engine::collect_garbage` hook. Public factories now
use explicit or inherited allocation contexts; Legacy facade/storage variants
and the private Legacy queue fallback have been removed. The original
baseline observations below describe the starting point, not the current branch.

## Measured runtime integration

Runtime source `8727ebcc4d809a126aadd9a59c2f993ca7a0c6dd` adds direct ordinary
own-data reads, including named String-object properties while retaining the
generic path for virtual length/indices, accessors and proxies. Its
[separate measurements](../benchmarks/runtime-gc/experiments/own-read-872)
return all66 correct workers/1056 checked values and qualify all33 actual-Boa
comparisons on local ARM64 guest CPU0, with time ratios0.356–0.739. Its
before/after comparisons againstefd are all within threshold: nominal shorter
times are not treated as a qualified improvement. Native658 tests, selected
fixed Test262, Clippy and three exact public integration Miri tests pass;
latest-source hosted and full library Miri checks are still separate gates.
The authored async comparison has2 faster and7 noise decisions, so it does
not qualify all three async workloads. Full application and SpiderMonkey
superiority remain unproved.

The later runtime source `efd0de57abca28fb00807f78e386bf94ef09f4b2`
records its own [original-eleven comparison and checks](../benchmarks/runtime-gc/experiments/own-write-efd).
On local ARM64 guest CPU0, all66 workers/1056 warmup and sample values are
correct and all33 comparisons pass the unchanged10%/MAD gate against the
actual pinned omoikane modified Boa. Time ratios range0.391–0.718. Both frozen
release workers use the same compiler; agent threads use guest CPUs1–3, and no
local build/test/profiler overlaps competitive timing. This is not host
physical-core isolation or evidence for arbitrary machines/applications.

The direct ordinary-property update removes repeated descriptor revival and
reconstruction only when the receiver is the same object and its existing own
data descriptor is writable. Heap ownership checks and strong-edge barriers
precede mutation; descriptor flags remain unchanged. Proxy/accessor, different
receivers, arrays, string/view/buffer objects and pending function prototypes
retain the generic path. Public integration tests cover cross-heap rejection
without mutation, live child edges across collection, replaced-child reclamation,
descriptor flags and different receivers, including two exact strict Miri tests.
The before/after comparison against413 records4 faster,29 noise,0 slower
decisions; all three prop-mono pairs take0.698–0.754 times the old duration.
This bounded result does not establish universal absence of regressions.

The following earlier records retain their original source identities and
unfavorable outcomes; they are not substituted for current-source qualification.

The runtime source at `4ca6287ce18e5e8baaf11f465ed923eca6496565` passes the
[original eleven-workload comparison](../benchmarks/runtime-gc/boa-comparison.json)
against omoikane's actual modified Boa through its public JsRuntime embedding.
The comparison pins omoikane to `007556adccb8fb4329236932ca8175f8e8d119d2`, uses
the original bodies, setup identities, counts and numeric oracles, and keeps
the same 512 MiB / 180-second process watchdog. Each case uses 15 samples in
three alternating process pairs on local Linux ARM64 guest CPU0, with the same
Rust compiler, release profile and inherited guest CPU affinity for both workers.
All 66 worker runs return the correct result. Every pair
passes the existing noise gate: improvement greater than both 10% and three
times the sum of relative median absolute deviations. Temiya's observed maximum
process high-water mark is 9076 KiB.

[Source verification](../benchmarks/runtime-gc/source-verification.json) records
the qualified source-tree objects and identical built/frozen worker. The
[strict public probe](../benchmarks/runtime-gc/ownership-probe.json) releases all
seven fixtures while preserving retained original and cloned handles.

The [unaffined comparison](../benchmarks/runtime-gc/experiments/4ca-unaffined-boa.json)
returned correct values but failed speed qualification: one prop-mono process
measured about 0.54 seconds rather than about 0.40 seconds in the other pairs.
The source of this process bimodality is not established. The controlled run
selects guest CPU0, the first allowed CPU, without a per-core speed search; it
does not pin host physical cores. Input bodies/counts, resource limits, oracles
and the existing noise gate are unchanged. No sample or pair was discarded.
The passing performance claim is limited to this controlled CPU0 condition.

The separate [main regression control](../benchmarks/runtime-gc/main-comparison.json)
compares the same candidate with frozen main `2d503cc`. Tracing introduces a
material throughput cost: several cases take 11–34% longer than main, despite
remaining faster than the pinned Boa. This is not a regression-free result.
Function call and prototype-method are faster than main; string-concatenation
differences remain within the comparison's noise threshold. The table gives
the range of process-pair median time ratios; smaller is faster.

| Workload | Temiya / omoikane Boa | Traced runtime / main |
| --- | --- | --- |
| arith | 0.64–0.64× | 1.14–1.16× |
| prop-mono | 0.76–0.81× | 1.32–1.34× |
| prop-mega | 0.53–0.54× | 1.24–1.31× |
| call | 0.41–0.41× | 0.64–0.65× |
| closure-alloc | 0.82–0.86× | 1.30–1.33× |
| object-alloc | 0.77–0.79× | 1.29–1.32× |
| string-concat | 0.39–0.42× | 1.01–1.07× |
| array | 0.63–0.64× | 1.31–1.32× |
| primitive-string-property | 0.52–0.52× | 1.13–1.13× |
| primitive-string-method | 0.67–0.68× | 1.11–1.26× |
| proto-method | 0.53–0.54× | 0.88–0.89× |

These are eleven microbenchmarks on one machine, not whole-application
compatibility, arbitrary JavaScript superiority, or a SpiderMonkey result.
GC reachability and rooted-handle safety have independent tests and probes;
correct benchmark values alone do not establish them. The full Miri gate is
partitioned from Miri's actual test listing, executes each assigned test exactly,
and requires every shard to pass; plan validation alone is not a Miri pass.

The [completed51d security record](../benchmarks/runtime-gc/ci-51d.json) verifies
all119 distinct library tests executed exactly once with zero skipped tests,
all eight shards and the aggregate passing, plus audit and ASan success.
It qualifies that source only; later optimization candidates require their own
checks rather than treating this earlier success as a current-source pass.

The later [completedefd record](../benchmarks/runtime-gc/ci-efd.json) likewise
independently verifies119 distinct exact library Miri executions with zero
ignored tests, all eight shards and aggregate, plus ASan and dependency audit.
It proves that source only; the direct-read runtime has its own validation.

### Hosted follow-up at revision51d9abf

[Run40723916807](https://github.com/ieee0824/temiya/actions/runs/40723916807)
records two independent x64 machines without selecting a CPU model or pinning
guest affinity. Both complete all four original-eleven report sets: candidate
versus main, candidate versus actual Boa, main versus actual Boa, and candidate
versus its identical executable. Every report has66 correct workers and1056
checked warmup/sample results. Independent recomputation validates all medians,
MADs, ratios, decisions, input oracles, complete coverage and alternating order.

Neither machine qualifies the entire candidate against Boa. On AMD EPYC9V45,
32 comparisons pass and one object-allocation comparison is within the noise
gate (ratio0.908, less than the required10% improvement). Main versus Boa passes
all33 comparisons; the identical-executable control has33 noise decisions.
On Intel Xeon6973P-C,24 candidate comparisons pass, seven are within noise and
two arithmetic comparisons are slower than Boa (ratios1.16–1.17). Main versus
Boa has31 passes and two noise decisions. Its identical-executable control has
28 noise, three faster and two slower decisions, so machine/process variability
is observable but does not excuse or erase the failed candidate qualification.

[All reports, build/compiler/machine metadata, progress and checksums](../benchmarks/runtime-gc/hosted-boa)
retain both hosts and all controls. The local CPU0 result above remains bounded
to its recorded condition. It does not prove the required speed goal on these
hosted machines; allocation and execution costs still need improvement.

### Separate allocation, root and pause observations

`embedding::runtime_workload_profile::original_eleven_runtime_allocation_root_and_collection_profile`
is an opt-in release test over the same pinned eleven inputs. Set
`TEMIYA_PROFILE_WORKLOADS` to the extracted fixture directory and
`TEMIYA_PROFILE_OUTPUT` to a new output path, then run
`cargo test --locked --release --lib embedding::runtime_workload_profile::original_eleven_runtime_allocation_root_and_collection_profile -- --ignored --exact`.
The test checks every result and records managed allocations, cumulative Root
registration/removal calls, peak live cells, current/peak distinct roots,
automatic collection count/time, explicit collection time, and the remaining
evaluation wall time. All roots are then released and live cells must reach
zero with balanced registration/removal counts.

These counters and pause timers compile only under `cfg(all(test, not(miri)))`.
They are absent from normal library builds, the competitive timing worker and
Miri. The instrumented single-pass timing is diagnostic, not speed acceptance;
its root counts refer to actual Heap Root registrations, not facade clones
sharing an existing registration. Collection pause time includes cache clearing,
sweep, retired host destruction and any destructor reentry. Peak counters include
Realm/bootstrap setup. Allocation counts cover managed cells, not Rust backing
allocations. Competitive measurements remain separate and uninstrumented.

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
control but retained all six cyclic/function cases. The function-allocation
update below records an intermediate reduction to three retained cases. The
tracing branch now exits zero with all seven fixtures released; the historical
failures are preserved rather than counted as collector passes. `tests/runtime_ownership.rs`
checks live-handle/clone/function/prototype safety and acyclic release; it does
not require known leaks to persist after migration. The strict command is the
post-migration reclamation acceptance test.

These probes cover runtime teardown. Live-Engine tests additionally run the same
root transitions with explicit collection before Engine teardown. The public
`Engine::collect_garbage` hook is available on the migration branch. Pending jobs, suspended generators,
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
| Engine / EngineBuilder (`embedding.rs`) | options, Box<dyn Any> host_state; Engine also owns shared Runtime | Engine owns the tracing heap through Runtime; opaque host state can own explicit external roots |
| RealmHandle / CompiledScript / ModuleHandle / ValueHandle / Exception | Realm / ScriptRecord / SourceTextModule / Value / thrown ValueHandle | R registered persistently with origin Runtime and existing provenance checks |
| HostCallContext / host arguments and results | IDs only in context; arguments, receiver and return handles have Values | Active R for call; validate result provenance before adopting its internal edge |
| FFI wrappers / registry (`ffi.rs`) | boxed Engine/Realm/Script/Value handles; registry stores kind/address only | Wrappers own R, release exactly once; address registry is not an extra GC root; CString errors have no Value edge |

Engine selects AST or verified bytecode execution; both paths' captures,
temporaries and active regions are included here. Experimental JIT, debugger
and standalone collector development components remain separate. Connecting
additional register/frame/cache state requires its own tracing audit
(#307, #324, #340).

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
persistent root table.

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
3. Initial safepoints use registered frames. This integration selects allocation
   safepoints and explicit collection; future loop-backedge/call-return sites
   must obey the same rooting protocol. Publish/root a new allocation before another
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

## Historical function allocation retention update

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

## 0.2 API migration

The public Engine now creates every Realm in its shared tracing context; the
private allocation-mode switch has been removed. Normal allocation safepoints
run every 4096 managed allocations. This initial interval is subject to the
original-workload memory and speed gates, not a measured optimum. Incoming
unpublished records participate in tracing before collection. Retired records
are destroyed outside the heap borrow, so host destructors can reenter runtime
allocation without resurrecting retired generations.

Each managed record is allocated once in shared storage used by its heap slot
and rooted facades. The heap keeps no separately allocated registration wrapper.
This shared storage does not determine liveness: only registered roots and
traced handles do. Slots are invalidated before retired storage is released.
Mutable access to shared records uses their explicit interior-mutable fields;
the standalone Heap's owned allocations retain scoped mutable access.

`Engine::allocation_context()` exposes the explicit context for host object and
native-function factories. A cloned context may outlive the Engine and request
collection through `AllocationContext::collect_garbage`. Contextless Object and
native factories have been removed. Cross-heap objects are rejected at Engine
and stored-edge boundaries. The version is now 0.2.0 for these Rust API changes;
this does not publish a release or change the opaque C ABI version.

Public collection, indexed, primitive-wrapper, Date and RegExp constructors now
require an explicit `AllocationContext`; collection `Default` implementations
have been removed. Descriptor materialization also requires the context.
`Realm::ordinary_in` is public for low-level callers sharing that heap.
The private `new_for` paths inherit the required object's context. Standalone JSON, JobQueue and Promise
factories now require context; parentless declarative/module Environments use
explicit `*_in(context)` factories, while child records require a parent.
VM blocks and cloned loop Environments require the parent guaranteed by the
bytecode verifier and inherit its allocation context.
Contextless Object/native factories and Object `Default` are now removed.
Object and WeakObject have only the traced representation. Standalone
`Realm::ordinary` owns a fresh tracing context, while `ordinary_in` shares an
explicit context. Native allocation callbacks can borrow their owner's context
per invocation without capturing Runtime ownership. All facade families now
hold rooted registrations in the shared heap, and all stored Object,
Environment, Realm, JobQueue and Module edges hold only typed handles.

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
cycles are reclaimed after the last external root is dropped. All runtime
objects now use this traced representation; allocation collection is enabled.

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
are unchanged by the ownership migration.

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

### Bounded activation facade region

The outermost VM run owns an external region with at most sixteen recently used
Object facades. Nested runs in the same Runtime share it. This reuses facade
allocations and root registrations for repeated reads of a stable receiver;
it does not store owning roots in managed records. Runtime holds only a Weak
reference to the region. Normal return, errors, and Rust unwind drop the region.

Fresh Object facades enter this same bounded region immediately after allocation,
so passing a new value through stored form does not destroy and immediately
recreate its first root registration. The capacity includes room for a container
and a small polymorphic receiver set. Neither rule changes the clearing protocol.

Object literals prepare their initial prototype edge before allocation. The
prototype supplies the allocation context; the unpublished record traces the
edge at the allocation safepoint even after its last public facade is consumed.
This removes the temporary global-object facade lookup and post-allocation
prototype write/barrier without changing prototype identity or initializer order.

The named-key candidate380471d removes runtime conversion of noncomputed String
property names. Its [local CPU0 records](../benchmarks/runtime-gc/experiments/named-keys-380)
show all three object-allocation comparisons18–21% shorter than the traced
predecessor and all33 actual-Boa comparisons faster beyond the original gate.
These results do not resolve the earlier Intel arithmetic qualification failure.

A Number-binding storage/inlining experiment at7fa1e21 passed native and targeted
Miri semantics but did not qualify its intended arithmetic improvement: all
three arithmetic comparisons were within noise, with two slower comparisons
elsewhere. Its [complete before/after record](../benchmarks/runtime-gc/experiments/number-leaf-7fa)
is retained and the implementation is reverted. The strengthened public API
test still checks signed zero and NaN payloads through assignment and reads.

The closure-allocation instruction profile also identifies repeated fresh
function initialization. AST and bytecode functions now prepare their internal
Function.prototype edge before allocation, using the same incoming-edge
protection as object literals. Fresh bytecode functions initialize their known
length/name and lazy prototype data descriptors directly, preserving attributes
and insertion order. The lazy prototype parent remains a validated strong edge;
descriptor values are Number/String/undefined leaves. Ordinary subsequent
property updates still use the validated descriptor path. This does not alter
the AST name-property behavior.

The private scalar-loop candidate keeps primitive local values in the active VM
scope instead of repeatedly resolving their Function Environment bindings. It
requires a fresh internal frame, Number arguments, mutable var bindings, a loop,
and a whole-body instruction proof excluding calls, properties, captures, outer
name lookup, implicit arguments and lexical bindings. Initially cached values
must be Number or undefined; the permitted instructions produce only primitive
values. There are no hidden managed edges or rooted objects in this cache.

The public `VirtualMachine::execute_in_function` path always updates the
caller-supplied Environment. It never assumes a host frame is private. Closure,
arguments, coercion and lexical-binding cases keep the ordinary path. Tests use
both public Engine backends, including duplicate/missing parameters, primitive
updates, signed zero/NaN payloads, captured reads, object coercion, immutable
bindings, and direct host inspection of a prepared frame after a loop.

Eligibility selects a separate const-specialized VM instantiation before
execution. The ordinary instantiation contains no scalar-cache checks in binding
operations, and its scope frames contain no extra scalar-cache field. Eligible
primitive locals belong to that private invocation until completion; the
whole-body proof forbids function-scope reentry and any escaping managed value.

Verified numeric-return calls resolve a captured Environment only when a guarded
captured binding is actually read. Parameter-only calls need no Realm or
Environment facade revival. The stored strong Realm/Environment edges remain
traced; this is not a weakening of captures. Failed guards reuse any loaded
Environment and enter the ordinary VM path once, with source-Realm error binding.

Both explicit collection and allocation collection clear this region before
tracing. Cached facades therefore cannot keep an otherwise dead key, cycle,
callback, or Realm alive through collection. Active operands, References,
function frames and the incoming unpublished record retain their existing
independent roots/traced edges. A bounded cache also avoids retaining a growing
history of allocated objects between collection points. Tests cover cache bounds,
slot retirement under allocation collection, explicit collection while a clone
remains live, nested region sharing and unwind cleanup.

Primitive property References in one VM invocation may also retain the five
primitive-prototype facades from that invocation's fixed, borrowed Realm.
The stored intrinsic registry is initialized once and does not change identity;
its prototype objects are already strongly reachable through the rooted Realm.
These are external VM-frame roots, never heap-owned roots or cached property
values. Current descriptors and prototype chains are read for every access, and
computed-key coercion stays at Reference consumption. All facade entries drop
on return or unwind. The public regression alternates all five primitive kinds
across two Realms, reenters a third Realm, collects during each outer iteration,
mutates an accessed property, and verifies zero live cells after releasing roots.

All five managed allocation families (Object, Environment, Realm, JobQueue,
SourceTextModule) now collect before allocation through a Runtime safepoint.
The normal production interval is 4096 managed allocations. Tests set
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
releasing the host token after the last handle. These fixtures now also pass
through the public default Engine and strict CLI, with the 0.2 context APIs and
without Legacy storage variants. The recorded external ABI/toolchain tests and
original-workload performance qualification are separate from the remaining
full Miri delivery gate.
