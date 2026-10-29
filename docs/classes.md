# Base class execution

The public Engine executes base class declarations and expressions through the
AST backend. Auto selects this path before execution when a Script contains a
class; explicit Bytecode still rejects class compilation.

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
created instance, while a primitive return leaves the instance in place.
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

Derived construction (#155), super references (#156), public fields (#157),
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
