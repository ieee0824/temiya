# Rust embedding API

`Engine::builder` owns configuration and optional host state. The state is
transferred into the engine and dropped with it. `Engine`, its realms, scripts,
and values are deliberately thread-confined (`!Send`/`!Sync`) because the
runtime currently uses `Rc` and `RefCell`; callers create one engine per thread
or perform their own message passing.

`create_realm`, `compile`, and `evaluate` return owned handles without exposing
runtime borrows. Handles carry engine/Realm provenance, and cross-engine use is
rejected. Compile failures use `EmbedError`; evaluation failures use an owned
`Exception`, distinguishing runtime error categories from JavaScript thrown
values. A returned `ValueHandle` owns its value and remains valid until dropped,
without borrowing the engine through a Rust lifetime.

On the 0.2 migration branch, all Engine Realms share one tracing heap. Allocation
safepoints collect unreachable graphs; `engine.collect_garbage()?` also requests
an explicit collection. Owned handles register roots, including handles kept
after the Engine is dropped. Dropping a Realm handle alone does not promise
immediate destruction while another owner keeps the heap alive.

Host objects with custom runtime hooks must use
`Object::new_in(engine.allocation_context())`. Low-level callbacks can use
`Object::native_function_in(engine.allocation_context(), callback)`; this
allocates in the shared heap without attaching a Realm's Function prototype.
Use `register_global_function` for the Realm-aware callback contract. Objects
from a separate `AllocationContext::new()` cannot be passed to this Engine;
the boundary rejects them instead of copying their identity or graph. The
contextless low-level constructors still use the legacy representation during
the migration and cannot supply objects to Engine APIs.

Opaque host callback captures are externally owned Rust values. Capturing an
owned JavaScript handle keeps its root alive; hosts must release their own
Rust ownership cycles. JavaScript edges stored by the runtime are traced.

Collection and indexed-object constructors now require an explicit context:
`MapObject::new(context)`, `SetObject::new(context)`,
`WeakMapObject::new(context)`, `WeakSetObject::new(context)`,
`ArrayObject::new(context, length)`, and `ArrayBuffer::new(context, byte_length)`.
The same rule applies to `StringObject::new(context, value)`,
`DateObject::new(context, milliseconds)`, `DateObject::now(context, host)`,
`RegExp::new(context, pattern, flags)`, and `RegExp::from_literal(context, spelling)`.
Number and BigInt wrappers use `NumberObject::new(context, value)` and
`BigIntObject::new(context, value)`. Standalone low-level Realm users can create
`Realm::ordinary_in(context)` to share the same heap with these objects.
`PropertyDescriptor::to_object(context)` creates the descriptor object in that
heap and rejects getter, setter, or value objects from another context.
The collection types no longer implement `Default`. Use the same context for
objects stored as keys, values, or elements. Weak keys become eligible for
removal after their last strong root is dropped and the context is collected.
TypedArray and DataView constructors continue to derive their context from
the required buffer, preserving the shared bytes and detachment behavior.

`Json::parse(context, source, reviver)` allocates its entire object/array tree
in the supplied heap. Reviver replacements at child and root boundaries must
also belong to that heap. `JobQueue::new(context)` has no `Default` counterpart.
`Promise::pending(context)`, `fulfilled(context, value)`, and
`rejected(context, error)` create fresh queues in that context; the existing
`*_on(queue, ...)` APIs use the explicitly supplied queue.

`Environment::declarative(parent)` and `Environment::module(parent)` require
an owning parent Environment and inherit its context. Use
`Environment::declarative_in(context)` or `Environment::module_in(context)`
for parentless records. Object, global and function Environment factories
derive their context from their required Object argument.

Explicit JavaScript throws keep their original value in `Exception::thrown`,
including throws that cross function or module boundaries. The handle retains
the engine and evaluation Realm provenance. A native language error keeps its
typed `ExceptionKind`; when materialized at a function boundary its error object
is also available through `thrown()`. An engine invariant failure remains
`ExceptionKind::Engine` and cannot be hidden by JavaScript catch/finally.

Host integration is exposed through `register_global_function`,
`create_object`/`set_global`, `load_module`, and the Realm-local job APIs.
Native callbacks receive owned, provenance-tagged handles and a
`HostCallContext` for creating results. A callback result from a different
engine or Realm is rejected. Host callback and resolver panics are caught at
the Rust boundary and converted to engine exceptions; they never unwind
through JavaScript evaluation. Jobs execute FIFO at an explicit `drain_jobs`
microtask checkpoint, and unhandled Promise errors can be observed with
`register_job_error_callback`.

JavaScript Promise rejection values retain their type and identity in reactions
and in `Exception::thrown()` passed to the job-error callback, including
`undefined`, `null`, NaN, signed zero, Symbols and objects. The Rust Promise API's
`reject_value` supplies an arbitrary reason without coercion. Existing unbound
Rust-only `RuntimeError` rejections preserve their diagnostic-string reaction
argument. A consumed settled rejection is marked handled at the synchronous
await boundary; pending-await continuations and the complete host rejection
lifecycle remain tracked separately.

## C ABI

`include/temiya.h` exposes ABI version 1 using opaque handles and explicit
`*_free` functions. Every fallible call returns `TemiyaStatus`; diagnostics are
owned `TemiyaError` handles. The ABI catches Rust panics and never lets them
unwind into C. `examples/embed.c` demonstrates version checking, compilation,
evaluation, and cleanup on every exit path.

All handles are checked against a type-aware live-handle registry before use.
Null cleanup succeeds, while stale, wrong-type, and double-freed handles return
`TEMIYA_INVALID_ARGUMENT` without being dereferenced. C embedders can expose a
reentrant unary number callback with `temiya_realm_define_number_function`;
callback failures become JavaScript exceptions. `scripts/check-c-abi` compiles
the public header/example and compares exported symbols with the ABI allowlist.

### Canonical module resolution

`Engine::load_module_with_resolver` adds a separate referrer-aware identity
resolver and source loader. The resolver receives `None` for the root, then the
canonical importing module name and original `ast::ModuleRequest` for each
static dependency. It returns the canonical cache/source name; import attributes
remain part of identity. Hosts should reject unsupported attributes and validate
specifiers before accessing files. Both callbacks are panic-isolated and handles
retain the existing engine/Realm checks. The original `load_module` callback and
per-specifier behavior remain unchanged. The [CLI](cli.md) uses this API for
nested relative imports and canonical file caching in one graph.
