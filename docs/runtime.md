# Runtime values

`temiya::runtime` models all eight ECMAScript language value types. Strings own
UTF-16 code units (including lone surrogates), Number uses binary64, BigInt is
arbitrary precision, and Symbol and Object comparisons use identity.

`Value` exposes the fundamental abstract operations needed by the interpreter:
`same_value`, `same_value_zero`, `to_boolean`, `to_primitive`, `to_number`,
`to_numeric`, `to_js_string`, and `to_bigint`. Object conversion hooks preserve
the observable `@@toPrimitive`, `valueOf`, and `toString` ordering and propagate
abrupt completions. These explicit bootstrap hooks remain the boundary until
Realm initialization installs the corresponding intrinsic properties.

Object storage currently uses `Rc`; this is an explicit pre-GC representation.
Tracing, rooting, weak references, and optimized tagged values are outside this
module's contract and will be introduced with the collector milestones.

`PropertyKey` keeps String and Symbol keys distinct and recognizes canonical
array indices. `PropertyDescriptor` models field presence independently from
field values and provides descriptor/object conversion. Ordinary objects expose
prototype-aware get, set, definition, deletion, membership, and key enumeration;
descriptor transitions enforce the ECMAScript non-configurable and non-writable
invariants. Own keys are returned as sorted array indices, insertion-ordered
strings, then insertion-ordered symbols. Prototype changes reject cycles and
respect non-extensibility.
