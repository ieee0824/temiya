# Garbage collector contracts

`temiya::gc` separates interpreter references from the collector algorithm.
`Handle<T>` is an opaque heap identity, not a Rust reference. A `Heap` resolves
it only inside `with` or `with_mut`, preventing heap borrows from being stored in
long-lived runtime state. Heap identity, slot, and generation checks make stale
or cross-heap handles fail safely.

`Root<T>` registers reachability with RAII. Cloning a root adds a registration;
dropping it removes one. Runtime state may store handles, but a handle that must
survive a collection point needs a live root or a traced path from one.
Root counts use slot-indexed entries with complete heap/slot/generation identity
checks. A dense list tracks distinct active roots, so registration and final
release avoid hashing, while collection visits active roots without scanning
the heap's historical slot count. Root clones retain separate registrations.

Types stored in the heap implement `Trace` and report every strong outgoing
handle through `Tracer::mark`. They do not trace weak edges and must not retain
the tracer. The current storage is non-moving, but no public API exposes an
address, leaving moving and non-moving collectors as compatible implementations.

`Heap::collect` is a stop-the-world tracing collector. It marks the complete
graph reachable from registered roots and sweeps unreachable slots, including
cycles. Reused slots receive a new generation so stale handles resolve to
`None`. `HeapOptions` controls the automatic allocation threshold and an
optional live-allocation limit. `try_allocate` runs collection before reporting
`OutOfMemory`, making both threshold and failure behavior deterministic in
embedders and tests.

Survivors are promoted to an old generation. When an old allocation gains a
young strong edge, the mutator calls `Heap::write_barrier`; the owner then joins
the remembered set scanned by `collect_minor`. Minor collection traces young
roots and remembered old owners, promotes surviving young values, and never
sweeps old values. Development and stress tests can call
`verify_write_barriers` to detect an old-to-young edge whose owner was not
remembered. Full collection remains the reference behavior for comparison.

Weak reachability uses `WeakHandle`, which upgrades only while its target slot
and generation remain live. Ephemerons retain their value only after the key is
marked; full collection repeats ephemeron processing to a fixpoint, which is the
primitive used by weak-keyed collections. `FinalizationRegistry` records only a
weak target plus an independent held value. Collection discovers dead targets
in registration order, sweeps them, and queues cleanup. The embedder invokes
`run_cleanup_jobs` afterward, so callbacks cannot resurrect swept targets and
no finalizer-at-process-exit guarantee is implied.

`Heap::stress` deterministically performs collection before every allocation.
`Heap::verify` checks root registrations, every traced edge, remembered owners,
and reusable slots, reporting stale-generation and cross-heap edges immediately.
`Heap::diagnostics` exposes live/root/free counts plus cumulative collections,
reclamation, and peak live allocations for tests and leak investigations.

Engine Realms share a tracing heap. Public Object, Environment, Realm, JobQueue
and Module facades register roots; stored graph edges contain typed handles.
Normal allocation safepoints collect every 4096 managed allocations, and
`Engine::collect_garbage` permits explicit collection. The runtime uses full
collection; standalone minor-collector support does not enable generational
collection in Engine.

The ownership graph, rooting protocol, public retention probe and delivery gates
are documented in [runtime ownership migration](runtime-ownership.md). The strict
public probe now releases all seven fixtures after the final handle is dropped,
while preserving values retained across Engine teardown. Separate live-Engine
tests cover allocation safepoints, jobs, suspended generators, module cycles,
weak collections, host reentry and cross-heap rejection.
