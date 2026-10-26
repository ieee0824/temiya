# Garbage collector contracts

`temiya::gc` separates interpreter references from the collector algorithm.
`Handle<T>` is an opaque heap identity, not a Rust reference. A `Heap` resolves
it only inside `with` or `with_mut`, preventing heap borrows from being stored in
long-lived runtime state. Heap identity, slot, and generation checks make stale
or cross-heap handles fail safely.

`Root<T>` registers reachability with RAII. Cloning a root adds a registration;
dropping it removes one. Runtime state may store handles, but a handle that must
survive a collection point needs a live root or a traced path from one.

Types stored in the heap implement `Trace` and report every strong outgoing
handle through `Tracer::mark`. They do not trace weak edges and must not retain
the tracer. The current storage is non-moving, but no public API exposes an
address, leaving moving and non-moving collectors as compatible implementations.
