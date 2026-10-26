# Runtime values

`temiya::runtime` models all eight ECMAScript language value types. Strings own
UTF-16 code units (including lone surrogates), Number uses binary64, BigInt is
arbitrary precision, and Symbol and Object comparisons use identity.

`Value` exposes the fundamental abstract operations needed by the interpreter:
`same_value`, `same_value_zero`, `to_boolean`, `to_primitive`, `to_number`,
`to_numeric`, `to_js_string`, and `to_bigint`. Object conversion hooks preserve
the observable `@@toPrimitive`, `valueOf`, and `toString` ordering and propagate
abrupt completions. The ordinary property/prototype model will replace these
bootstrap hooks as that layer lands.

Object storage currently uses `Rc`; this is an explicit pre-GC representation.
Tracing, rooting, weak references, and optimized tagged values are outside this
module's contract and will be introduced with the collector milestones.
