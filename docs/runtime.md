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
