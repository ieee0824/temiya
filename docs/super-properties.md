# Super property references

Ast and Auto execute `super.name` and `super[expression]` in object methods,
class constructors, instance methods and static methods. An arrow captures the
owning method's environment and receiver. An extracted method keeps its original
HomeObject, including when it is called with a different receiver.

Each reference reads HomeObject's current prototype after evaluating a computed
key expression. Get and Set use that captured base with the method's actual
`this` as receiver, including accessors and Proxy traps. Changing the prototype
therefore affects later references; key conversion or the assignment's right
side cannot retarget a reference that was already created.

An uninitialized derived `this` throws before evaluating the property expression.
Key conversion occurs when GetValue or PutValue consumes the reference, and the
converted key is retained for compound assignments and updates. Deleting a
super reference throws ReferenceError before key conversion, including when its
base is null. Reads and writes with null bases throw TypeError.

Run `cargo test --locked --release --test super_properties --test derived_classes`
for public Engine integration tests in both Ast and Auto. They cover receiver
identity, inherited accessors, detached methods, dynamic parents, nested arrows,
key evaluation and conversion, strict assignment failures, Proxy traps and
reentrant garbage collection of captured HomeObjects and receivers.

This connection uses the AST function path. Explicit Bytecode does not compile
super-dependent methods. Fields/private elements, eval and general async or
parameter-environment gaps have separate issues. Fixed upstream results and
performance qualification must be recorded before treating #156 as complete.
