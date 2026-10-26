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

`Environment` represents Declarative, Object, Global, Function, and Module
Environment Records with explicit outer links. Declarative bindings retain TDZ,
mutability, strictness, and deletability state. Object records support `with`
visibility through a Realm-supplied `Symbol.unscopables` key. Global records
separately track lexical and `var` declarations and expose declaration-conflict
checks. Function records own `this`, function-object, and `new.target` state;
module import bindings are immutable live references without performing module
linking.

`Realm` owns the global object and Global Environment Record. `ScriptRecord`
owns parsed code and its Realm, while `ExecutionContext` owns the active Realm,
code/function association, and lexical/variable environments. A single-thread
`Agent` exposes explicit enter, suspend, resume, and leave transitions. Entering
a nested context suspends its caller, leaving resumes it, and `run_in_context`
restores the stack for both normal and abrupt ECMAScript completions.

## JavaScript exceptions

`RuntimeError::thrown` carries an owned `Value` across function, accessor,
constructor, module, and generator call boundaries. It preserves identity,
`null`, `undefined`, NaN, and signed zero without coercing the value to a string.
`ThrownValue` is distinct from native TypeError/RangeError/ReferenceError/
SyntaxError failures and from an engine `InternalError`.

The AST interpreter catches both explicit throws and native language errors.
It runs `finally` after an error from either the try block or its catch handler;
an abrupt finalizer replaces the pending JavaScript completion. Internal engine
failures bypass JavaScript catch/finally rather than being swallowed by them.
Native language errors are materialized with intrinsic error prototypes; a
replaced global constructor is not invoked. A JavaScript function retains its
own Realm's native error object when returning an error to a caller.

This does not complete Error constructor APIs, Promise rejection reactions,
generator suspension/handler resumption, or bytecode exception handlers. Those
remain separate work. The Error-family prototypes introduced here provide the
identity/name/message needed by catch; constructor message/cause behavior and
the remaining Error APIs still need implementation.

The focused upstream regression check reads unmodified `sta.js`, `assert.js`,
and seven try/catch/finally tests from Git objects at the pinned Test262 revision:

```sh
TEST262_DIR=/tmp/temiya-test262 scripts/test262
TEST262_DIR=/tmp/temiya-test262 cargo test --locked --test exception_flow \
    pinned_test262_exception_regressions --all-features -- --ignored --nocapture
```

The initial result is 14 passing variants (sloppy and strict), zero failures and
zero skips within this selected suite. Other Test262 cases are unmeasured, not
passes. A normal `cargo test` reports the corpus-dependent test as ignored;
the command above and CI explicitly run it. The behavior follows
[TryStatement evaluation](https://262.ecma-international.org/17.0/#sec-try-statement-runtime-semantics-evaluation).

## Array stack and queue methods

Array literals and the Array constructor share the Realm's Array prototype,
which is itself an empty Array exotic object. `push`, `pop`, `shift`, and
`unshift` are callable from JavaScript and generic over array-like receivers.
They box primitive receivers, read/convert length once, preserve holes and
inherited indices, and perform observable Get/HasProperty/Set/Delete operations
in specification order. Failed writes/deletes throw and keep preceding changes.
Array-like lengths use the 2^53−1 limit without wrapping to a 32-bit index;
actual Arrays still enforce their separate 2^32−1 length limit.

Length conversion invokes JavaScript `Symbol.toPrimitive`, `valueOf`, and
`toString` methods; explicit Rust embedding conversion hooks remain supported.
Object literals now inherit Object.prototype, whose valueOf/toString fallback
methods support this conversion. The Symbol.toPrimitive/toStringTag identities
and Number safe-integer constants are exposed for these operations. This does
not complete the Symbol/Boolean/Error constructor APIs or replace the bootstrap
conversion path used by every other operator.

The focused Test262 gate uses 25 unchanged files / 50 strict and sloppy variants:

```sh
TEST262_DIR=/tmp/temiya-test262 cargo test --locked --all-features \
    --test array_stack pinned_test262_array_stack -- --ignored --nocapture
```

An audit of all 89 upstream files in the four method directories found 69 files
passing both variants (138 passes), 20 failing files, and zero timeouts. Twelve
fail before the test body in propertyHelper.js because other builtin function
APIs are incomplete; eight need Object.freeze. These failures remain tracked
by #202/#146 and #208 and are not counted as passes. The failing first variant
stops that file's audit, so its second variant is unexecuted. Method descriptors,
read-only properties, and partial updates are separately covered through the
public Engine API in `tests/array_stack.rs`. To reproduce an individual audit
case, set `TEST262_CASE=push/length.js` when running the command above.
The [audit manifest](../test262/array-stack-audit.json) records every file's
outcome and the relevant follow-up Issues for failures.

The stack/queue algorithms use constant auxiliary space, with no precomputed
property snapshot. Array element storage may change representation during a
write or deletion as described below; ordinary named properties still use
linear searches.

## Array element storage

Array indices (0 through 2^32−2) use a separate packed vector, holey vector, or
ordered sparse map. Canonical index parsing, complete property descriptors,
prototype lookup, and Proxy dispatch remain shared with ordinary property
operations. A hole is distinct from a present `undefined`; named and Symbol
properties retain their insertion order after ascending indices.

Large gaps switch to sparse storage without allocating up to the index. Dense
storage returns at 50% occupancy (small spans of at most 64 slots are allowed);
deletion switches holey storage to sparse below 25% occupancy. Filling all
holes returns to packed storage. These thresholds are implementation choices,
not observable JavaScript state. The current backing store keeps full
descriptors, rather than specializing number/value-only elements. Clearing a
large backing vector releases excess capacity; cycles in stored values retain
the existing Rc ownership limitations.

Numeric length shrink deletes existing elements in descending order, stopping
at a nonconfigurable property and restoring length to that index plus one.
Invalid descriptor attributes reject the operation before deleting elements;
a requested read-only length takes effect even after a partial shrink. No
getter or Proxy trap is bypassed: Array's own deletion operation has no user
callback, and a Proxy still dispatches through its traps. Huge sparse lengths
do not cause a loop across missing indices.

`tests/array_elements.rs` covers the public Engine and Rust object APIs,
including representation transitions, inherited accessors that mutate storage,
Proxy traps, descriptor failures, key order, owned-value release, and 4,000
mixed operations compared with an independent ordered model. A private test
also checks actual representation transitions and retained capacity. CI runs
18 additional variants from the pinned Test262 Array/length directory:

```sh
TEST262_DIR=/tmp/temiya-test262 cargo test --locked --all-features \
    --test array_stack pinned_test262_array_elements -- --ignored --nocapture
```

The complete 30-file Array/length audit passes 23 files (46 variants); seven
files fail and their second variants are unexecuted. Five involve existing
non-number ArraySetLength coercion gaps (#450), one needs isPrototypeOf (#209),
and one needs the cross-Realm Test262 host (#147). This storage change does not
claim complete ArraySetLength conformance. See `test262/array-elements-audit.json`
for exact outcomes; select a single file with `TEST262_CASE=filename.js`.

## String indexing and slicing

The public `Engine` AST path exposes `String.prototype.charAt`, `charCodeAt`,
`codePointAt`, `at`, `slice`, and `substring`. These follow
[ECMAScript 2026 section 22.1.3](https://tc39.es/ecma262/2026/multipage/text-processing.html#sec-properties-of-the-string-prototype-object).
Storage and indexing use UTF-16 code units, including lone surrogates;
`codePointAt` combines only a valid leading/trailing surrogate pair.
Negative positions are relative only for `at` and `slice`. `substring` clamps
negative positions and swaps its normalized endpoints. Out-of-range results
are `''`, `NaN`, or `undefined` according to the particular method.

Primitive property reads consult the current Realm's wrapper prototype after
checking a string's own length/index properties, retaining the primitive as the
accessor/call receiver. Reads do not allocate a wrapper object. Canonical string
indices are distinguished from names such as `'01'`, `'+1'`, and `'-0'`.
Primitive writes and standalone `Reference::property` values without an
associated Realm retain their earlier limited behavior.

Methods coerce the receiver before indices, invoke actual JavaScript
`Symbol.toPrimitive`/`toString`/`valueOf` methods, and preserve thrown values.
Explicit Rust conversion hooks retain their embedding behavior. String wrappers
and the empty `String.prototype` carry String data; `valueOf` and `toString`
check that brand and reject a Proxy around a wrapper. `String()` defaults to an
empty string, and String call/construction observes JavaScript coercion.
The String call alone accepts a Symbol primitive as a descriptive string.
Derived construction/new-target behavior remains incomplete.

`tests/string_methods.rs` checks UTF-16 boundaries, metadata, coercion order,
exceptions, primitive receivers, and boxed strings through the public Engine.
The focused upstream selection runs 28 unmodified Test262 files in both modes
(56 variants) with original harness includes:

```sh
scripts/test262
TEST262_DIR=target/test262 cargo test --locked --test string_methods \
  pinned_test262_string_methods -- --ignored --nocapture
```

Set `TEST262_CASE=charAt/pos-rounding.js,slice/this-value-not-obj-coercible.js`
to run an explicit selection from `test/built-ins/String/prototype/` at the pinned
revision. The complete six-directory audit is recorded in
[`string-methods-audit.json`](../test262/string-methods-audit.json): 123 files / 246
variants passed; 43 files failed and their later 43 variants were unexecuted. External
builtin/prototype/harness failures are failures, not skips or passing tests;
each file stops at its first failing variant, so later variants remain unrun.
This is not full String conformance: search, padding, case conversion, iteration,
RegExp integration are separate work; legacy Annex B `substr` is tracked in #454.

## Compact UTF-16 storage

`JsString` stores short values inline and shares immutable longer buffers with
`Arc<Vec<u16>>`. The inline capacity is ten UTF-16 code units on 64-bit targets
and four on 32-bit targets; no mutable buffer access is exposed. Cloning a long
string shares its allocation. `Send`/`Sync`, exact lone-surrogate preservation,
content equality/hash/order and the public `code_units`/conversion methods are
retained. `from_code_units_slice` also avoids an intermediate Vec for short
literal and indexed-character evaluation. Equality and ordering use the active
UTF-16 slice rather than the storage variant or buffer identity.

The Arc owns a Vec so a newly constructed long result can retain its buffer;
converting an existing Vec into a flat Arc slice would copy that buffer. This
choice still uses separate Vec-buffer and Arc-header allocations for a fresh
long value. Concatenations of at least 256 code units can instead share their
operands in immutable Arc rope nodes. Short concatenations use inline storage
or one exactly sized flat buffer. Empty operands reuse the other operand.
Rope depth is bounded at 32; exceeding the bound copies into one flat buffer,
bounding recursive traversal and destruction even for repeated append/prepend.
This still has quadratic worst-case copying for sufficiently long chains; it
does not provide a balanced rope's asymptotic guarantee.

`len` and `is_empty` use stored lengths. Contiguous access through `code_units`
lazily fills a thread-safe `OnceLock` cache; it traverses uncached descendants
without filling their caches, avoiding overlapping intermediate copies.
Equality/hash/order and slices still use exact UTF-16 contents. Rope nodes
contain only immutable strings, so their ownership graph has no cycles or
managed Object edges. Identifier atoms remain unimplemented. GC integration and
identity testing across actual Engine collections remain outstanding in #257
and #415; the string buffer contains no managed Object edges.

`tests/compact_strings.rs` covers UTF-16 lengths across both storage boundaries,
NUL/non-BMP/lone surrogates, borrowed input mutation, clone/thread lifetimes,
content hash/order, distinct Symbols, and public Engine alias/slice/property-key
behavior. String storage remains the former Vec footprint (24 bytes on the
measured 64-bit targets). Rust layout is not a public ABI promise.

Run the separate allocation probe and budget checks with:

```sh
cargo run --locked --release --example string_allocation_probe > allocations.json
node scripts/check-string-allocations.mjs allocations.json
```

The single-thread probe warms each operation before counting allocator requests,
including reallocations. Its reference Vec uses the former UTF-16 construction
and clone operations. It reports allocation requests/bytes, not elapsed time or
RSS. Timing workers do not include the counting allocator. The observed
[allocation report](../benchmarks/compact-string-allocations.json) is separate
from full Engine throughput comparisons.

The [concatenation allocation observation](../benchmarks/string-concat-allocations.json)
compares the previous interpreter's copy/extend path with `concat`, including
final contiguous access. Across 100 chains of 128 sixteen-unit appends, requested
bytes fell from 78,544,000 to 2,868,000; allocation requests fell from 38,300 to
14,500 on both measured Rust toolchains. Two two-unit operands concatenate
without allocation. These standalone counts include allocator requests, not
retained memory, RSS, copying instructions, or Engine throughput. The existing
CI allocation check also rejects regressions in these concatenation budgets.
