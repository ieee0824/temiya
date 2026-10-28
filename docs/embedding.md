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
