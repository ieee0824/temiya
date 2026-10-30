# Class execution

The public Engine executes base and derived class declarations and expressions through the
AST backend. Auto selects this path before execution when a Script contains a
class; explicit Bytecode still rejects class compilation. Within an Auto
AST Script, eligible simple-parameter ordinary and class function bodies are
independently verified and run by the bytecode function runtime. Explicit Ast
keeps function bodies in the AST interpreter. Unsupported bodies select AST
before execution; runtime errors never trigger a second execution.

The per-evaluation cache shares immutable code, not class environments, `this`
or objects. Span/kind lookup is checked against the complete function AST, so
unrelated definitions cannot reuse code by coincidental source offsets. Code
holds only weak cache links; class instances and captured names retain the
normal traced runtime ownership.

```js
class Point {
  constructor(x) { this.x = x; }
  read() { return this.x; }
  static make(x) { return new Point(x); }
}
Point.make(42).read(); // 42
```

Declarations create lexical bindings with a temporal dead zone, including for
`typeof`. The outer declaration binding is mutable and is not a global-object
property. A named class also has a distinct immutable inner binding captured by
its methods. Anonymous class variable initializers and identifier assignments
infer their names. Class code executes strictly.

Both explicit and default constructors support `new`; direct calls throw
TypeError before executing the body. A returned object replaces the newly
created instance. For base constructors, a primitive return leaves the instance
in place; derived return rules are described below.
Constructor bodies observe their actual `new.target`. Ordinary/static methods,
computed names and accessor pairs retain their descriptors and source order.
Methods are non-enumerable; the constructor's `prototype` property is
non-writable and non-configurable. A computed static method cannot overwrite
that property: a failed definition throws TypeError.

Classes inherit restricted `caller` and `arguments` accessors from their Realm's
Function.prototype. Both accessors use the same non-extensible TypeError thrower;
classes do not gain own properties for these names. This follows
[AddRestrictedFunctionProperties and %ThrowTypeError%](https://tc39.es/ecma262/2026/multipage/ordinary-and-exotic-objects-behaviours.html#sec-addrestrictedfunctionproperties).
Definition evaluation follows the selected
[ECMA-262 2026 class algorithm](https://tc39.es/ecma262/2026/multipage/ecmascript-language-functions-and-classes.html#sec-runtime-semantics-classdefinitionevaluation).

Non-strict ordinary functions retain a redacted legacy extension: their own
`caller` and `arguments` properties return null, are non-writable,
non-enumerable and non-configurable, and never expose stack frames. They are
materialized only on reflection/access so normal function creation and calls
do not allocate these extra properties. Strict functions, methods, arrows,
generators, async functions and classes do not receive this extension. This
does not implement general legacy caller introspection.

Super-property references (#156), public fields (#157),
static initialization (#158) and private elements (#159/#160) are separate work.
Unsupported elements produce explicit Engine failures; they are never silently
dropped. General parameter-environment behavior (#165), arrow new.target (#168)
and eval (#169) also remain incomplete. No whole-class or whole-corpus conformance
claim follows from the focused examples.

Run the actual Engine integration and existing related regressions with:

```sh
cargo test --locked --test base_classes --test functions_runtime \
  --test statement_evaluation --test object_function_reflect
```

The class tests execute AST and Auto and include TDZ, immutable inner names,
default/explicit constructors, strict calls, descriptors, computed names and
GC-surviving class environments and instances. Fixed Test262 originals retain
their own recorded selection and every outcome; their results do not qualify
the separate incomplete features above.

## Derived construction

Heritage connects both the constructor and instance prototype chains. Explicit
derived constructors begin with uninitialized `this`; `super()` constructs with
the original `new.target` and then binds that environment exactly once. Arguments
run before the super-constructor validity check, and a duplicate super call
performs construction before rejecting the second binding. An object return can
override the instance even before super; undefined requires initialized `this`,
and other primitive returns throw TypeError. Null heritage has a null instance
prototype parent and requires an explicit object-returning constructor.

An implicit derived constructor forwards the argument List directly, without
consulting an array iterator. Arrows created before super retain the owning
`this` binding and observe its later initialization. Captured environments are
traced through stored closures and suspended evaluators. Derived bodies use
the AST evaluator in both Ast and Auto; current leaf/activation-elision paths
do not model uninitialized constructor `this`. This does not connect super
property references, fields/private elements or complete builtin subclassing.

Run `cargo test --locked --test derived_classes --test base_classes` for the
public Engine tests. Fixed upstream coverage retains every recorded dependency
failure separately and does not establish full class conformance.

Derived constructors omit the implicit arguments object only when a complete
syntax walk finds neither arguments nor eval identifiers, including parameter
defaults and all nested functions/classes. The immutable result is cached with
the function code before execution. Observable arguments, including captured
arrows and extra actual parameters, keep the existing binding and strict
unmapped values. This changes allocation only; it neither skips body effects
nor retries errors.

Instance allocation reads the actual new target's `prototype` once. A
non-object result selects the appropriate intrinsic prototype from that
constructor's Realm, including through bound functions and proxies; a revoked
proxy throws. Allocation errors before entering an ordinary constructor belong
to the caller's Realm, while errors from the constructor body retain its Realm.
Subclassing Object allocates a fresh instance instead of returning its argument.

The existing Array, Number, String, Map/Set/weak collection, ArrayBuffer,
DataView, Date and RegExp constructors preserve their implemented payloads
with the derived prototype. Argument conversion and prototype access keep their
constructor-specific order. DataView validates the initial range before
prototype access and rechecks detachment afterwards, following the
[DataView constructor algorithm](https://tc39.es/ecma262/2026/multipage/structured-data.html#sec-dataview-buffer-byteoffset-bytelength).
This does not complete missing collection iteration, concrete typed-array
globals, or placeholder Boolean/Error constructor payload behavior. The
integration tests distinguish prototype propagation from payload coverage.
