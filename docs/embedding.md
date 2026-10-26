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

Host integration is exposed through `register_global_function`,
`create_object`/`set_global`, `load_module`, and the Realm-local job APIs.
Native callbacks receive owned, provenance-tagged handles and a
`HostCallContext` for creating results. A callback result from a different
engine or Realm is rejected. Host callback and resolver panics are caught at
the Rust boundary and converted to engine exceptions; they never unwind
through JavaScript evaluation. Jobs execute FIFO at an explicit `drain_jobs`
microtask checkpoint, and unhandled Promise errors can be observed with
`register_job_error_callback`.
