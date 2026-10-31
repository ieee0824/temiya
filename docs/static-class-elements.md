# Static class elements

Ast and Auto evaluate public static fields and static blocks after computing all
class element keys and installing methods. The immutable inner class binding is
initialized before static initialization begins. Static fields and blocks then
run in declaration order, once per class evaluation.

Static initializers receive the constructor as this. Their super properties use
the constructor's current prototype, and new.target is undefined. Static blocks
have separate strict function environments for their variables; closures retain
that environment. Field initializers use named evaluation for anonymous
functions and classes, including computed Symbol property names.

Fields define writable, enumerable, configurable own data properties and bypass
inherited setters. Initializer exceptions and failed property definitions stop
later static elements. All instance field definitions are available before a
static initializer constructs an instance of the class.

Pending initializer functions remain rooted while earlier initializers call
JavaScript or run garbage collection. tests/static_class_elements.rs verifies
reentrant collection and that the graph is reclaimed when the Realm is dropped.
Private fields and methods remain a separate unsupported boundary.

The complete fixed official selection has 276 files and 547 variants per
backend. Its pre-implementation results are 112 passes, 431 failures and four
timeouts. Final official-suite and performance qualification are pending. The
five authored workloads and pinned Boa oracle probes were frozen before runtime
implementation under benchmarks/static-class-elements; rejected inputs remain
recorded. No complete class conformance or unconditional speed claim is implied.

Anonymous default-exported classes use the module default binding while their
constructor name is set to default before static initialization. The ModuleGraph
integration test checks the field/block observations and that no source-visible
default binding is introduced.
