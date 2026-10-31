# Public instance fields

Ast and Auto initialize public instance fields in source order. Computed names
are converted once when the class is evaluated; initializer expressions run for
each instance. A field with no initializer defines an own undefined value.
Properties are writable, enumerable and configurable data properties, so an
inherited setter is not invoked. Failed definitions and initializer exceptions
stop later fields.

Base fields run before constructor parameter defaults and the body. Derived
fields run immediately after a successful super() binds this, including calls
through arrows. Default-derived and delegated constructors initialize their own
fields after constructing the parent. Returning a different object before super()
does not initialize the derived fields.

An initializer has its own strict function environment, this receiver and
undefined new.target. Arrows retain that receiver and the class prototype as
HomeObject for super properties. Initializer code, defining environments and
HomeObjects are traced from the constructor. Public Engine tests in
`tests/instance_fields.rs` cover reentrant collection and reclamation.

Static fields, static blocks and private elements remain unsupported and are
rejected before class effects. Eligible function-body declarations enter the VM
through the shared class evaluator with their lexical TDZ and mutable outer
binding. Complete syntax analysis permits loop-header reuse only when fields,
keys, defaults and nested functions/classes cannot observe any cloned header
name or eval. Capturing headers and per-iteration body bindings remain distinct.
This implementation does not establish complete
class-fields-public conformance or speed qualification; the fixed 2057-file
selection and five pre-implementation Boa oracle probes are retained under
`benchmarks/instance-fields` while runtime comparison is pending.

The initial3f86f24 snapshot retains all4098 variants per backend:579 prior passes
are preserved and429 new variants pass;2294 failures and796 timeouts remain.
Both backend outcome lists match. Its initial native comparison has5 significant
Boa wins and10 noisy comparisons, so the suite is not speed-qualified. It also
inherits the separately fixed namespace regression. Run
`node benchmarks/instance-fields/verify-initial.mjs` to replay that history.

The following declaration/capture optimization passes825 native tests. The
allocation test fails before the proof with40 versus13 live allocations for
thirty versus three iterations, then passes with a constant allocation count.
Delayed initializers, nested arrows/defaults/classes and distinct body bindings
retain their values. Fresh performance and upstream qualification remain pending.

Auto shares immutable function code created by initializers while keeping
instance environments separate. Zero-parameter synchronous arrows consisting
only of this.property can read an object receiver without an extra activation.
They resolve the captured this binding at call time, preserve getter/Proxy
dispatch and exception identity, and retain the ordinary path for primitive
receivers, defaults, async functions and other bodies.
