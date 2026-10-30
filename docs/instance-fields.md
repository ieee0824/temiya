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
rejected before class effects. Field-bearing classes remain excluded from the
VM loop-header reuse proof. This implementation does not establish complete
class-fields-public conformance or speed qualification; the fixed 2057-file
selection and five pre-implementation Boa oracle probes are retained under
`benchmarks/instance-fields` while runtime comparison is pending.
