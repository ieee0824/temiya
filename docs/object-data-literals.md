# Object initializer data properties in AST and bytecode

The bytecode compiler lowers ordinary and shorthand data properties, computed
keys, numeric property names, object spread and the literal `__proto__` setter.
It creates a fresh object with the Realm's Object prototype, converts each
computed key before evaluating its value, and defines properties in source
order. Repeated ordinary keys replace values. Computed and shorthand
`__proto__` names remain ordinary properties; a colon property changes the
prototype only for an Object or null value.

AST and VM share the property-key conversion, data definition and object-copy
operations. Spread snapshots the source's own keys, checks each current own
descriptor, and reads only enumerable values. Symbol keys and getters are
included; inherited values and nonenumerable properties are not copied.
Proxy errors from ownKeys, getOwnPropertyDescriptor and get propagate and stop
later evaluation. String primitives contribute enumerable UTF-16 indices.
Object copy does not use the iterator protocol.

The instructions are `CreateObject` (+1 operand), `CoercePropertyKey`
(replace one operand), `DefineObjectProperty` (object/key/value to object),
`DefineNamedObjectProperty` (object/value to object using a String constant),
and `SpreadObject` (object/source to object). They require an active verified scope
and sufficient operands. Runtime target checks also reject malformed operand
values. No AST evaluator is used by these VM instructions.

Noncomputed identifier and String names use `DefineNamedObjectProperty` without
materializing and coercing a key Value. The verifier requires a valid String
constant, and runtime definition still uses the shared data/prototype operation.
Computed and numeric names keep their existing conversion path before the RHS.
Shorthands, duplicate keys, UTF-16 names and the literal prototype setter retain
their original evaluation order and meaning. This removes repeated String-key
conversion observed in the tracing-branch object-allocation instruction profile;
native speed qualification is recorded separately in the runtime migration.

Object methods, accessors and their HomeObject/super behavior are still rejected
by this bytecode lowering. Function name inference for anonymous data-property
values is also incomplete. These remaining parts of #289 stay open. Existing
AST methods retain their own path. This change does not complete array/call
iterator spread (#172), Engine integration (#307), or Boa superiority (#427).

The focused tests compare AST and VM for duplicate/shorthand/numeric keys,
prototype setters, observable computed coercion order, enumerable symbols and
getters, and abrupt Proxy operations. Five original Test262 prototype files
run in both strict/sloppy variants under AST and explicit VM, with original
sta/assert harnesses evaluated by AST. No case is skipped or redirected to AST
when VM compilation fails. This scoped set does not establish full object
initializer conformance.

Primary semantics: [Object Initializer](https://tc39.es/ecma262/multipage/ecmascript-language-expressions.html#sec-object-initializer)
and [CopyDataProperties](https://tc39.es/ecma262/multipage/abstract-operations.html#sec-copydataproperties).

## Default AST performance observation

[Hosted run 40472024686](https://github.com/ieee0824/temiya/actions/runs/40472024686)
compared the default AST worker before/after on AMD EPYC 9V45, with the original
11 workloads, 15 timed samples plus a checked warmup, three alternating pairs,
and 512 MiB / 180-second guards. Both modes have 66 successful workers and 1056
correct values. [Before/after](../benchmarks/object-data-hosted-beforeafter.json)
has all 33 pairs within noise; [actual Boa](../benchmarks/object-data-hosted-boa.json)
has all 33 pairs slower for Temiya. This records the effect on the default AST
path, not VM throughput or overall Boa superiority. The
[provenance record](../benchmarks/object-data-hosted-provenance.json) retains
source/binary/build identities, machine data and hashes. All sample statistics
and fixed classifications were independently recomputed.
