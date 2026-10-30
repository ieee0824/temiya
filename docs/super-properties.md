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
Writes inspect the receiver's own descriptor and propagate Proxy or module
binding errors, including a namespace export's temporal dead zone.

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

Super references use the shared AST expression evaluator. Methods with direct
super references select AST before execution. Eligible methods that contain
class expressions can run surrounding code in the VM: the CreateClass bridge
uses the enclosing method environment for super in class heritage and computed
keys, including its actual receiver and current HomeObject prototype. This
bridge does not introduce general super-reference bytecode instructions. Fields/private elements, eval and general async or
parameter-environment gaps have separate issues. Fixed upstream results and
performance qualification must be recorded before treating #156 as complete.

AST ordinary and method calls allocate the implicit arguments object only when
it can be observed. The conservative complete-syntax scan also checks parameter
defaults, nested functions/arrows, class heritage and computed keys, and eval.
Calls that can observe arguments retain the actual object and its normal traced
ownership. This does not change the separate parameter-aliasing scope.

At candidate `b1557a5`, the fixed 310 upstream files have 601 variants in each
of Ast and Auto: 405 pass and 196 fail, with no skipped, timed-out or missing
variants. All previous 292 passes remain passing; 113 additional variants pass.
The [dependency audit](../benchmarks/super-properties/dependencies-v1/audit.json)
retains every failed diagnostic and source hash. It records unsupported class
elements, eval, async generators, missing native APIs, generic iterator/spread
behavior and the separate import-defer proposal-adoption decision (#541).
Twenty-four authored class-free probes per backend confirm the recorded missing
behavior; they are not additional conformance passes. Later fixture assertions
remain unproven. Run `node benchmarks/super-properties/verify-dependencies.mjs`
to replay the retained records and classification checks.

Eligible non-strict straight-line functions and object accessors can also use
the existing leaf executor when their activation cannot be observed. The caller
normalizes this with the defining Realm before executing the leaf plan; property
references retain their original strictness, including silent failed writes in
non-strict functions. Unsupported bodies enter their normal activation before
executing any source effects, and a leaf runtime error is never retried.

Eligible function bodies can also create object literals containing methods and
accessors through verified CreateObjectLiteral metadata. The shared evaluator
receives the current VM environment and containing strictness; it preserves
computed-key order, spreads, prototype setters, captures and HomeObjects while
surrounding supported loops run in the VM. Capturing initializers prevent scalar
binding elision and unsafe loop-environment reuse. Definitions use immutable
AST metadata and create fresh runtime objects, with no retry after an error.
Top-level object methods remain outside the whole-Script compiler subset. This
does not introduce general super-reference bytecode instructions or complete
#299.

A method or arrow whose complete body only returns a named super-method call
can omit its own activation when every formal parameter is simple and every
forwarded argument is one of those parameters. Missing arguments become
undefined; reordered arguments and omitted extra actuals follow the explicit
argument list. Defaults, spread, computed keys, extra statements, async/generator
bodies and constructors retain the ordinary path. Arrow calls resolve the
owning function's live this binding, including the uninitialized derived case.
The current HomeObject parent, getter/Proxy behavior, receiver and call-depth
limit remain observable. Errors from property lookup or the target are never
retried through the AST evaluator.

Source df1a206 retains 120 native worker runs and 1920 checked values in
`benchmarks/super-properties/super-delegation-v1`. Object/class/arrow super-call
workloads each have three significant wins against both Boa and the previous
runtime; the same-binary control has fifteen noisy comparisons. The accessor
and computed-update workloads remain noisy, so the complete issue is still not
speed-qualified. Run `node benchmarks/super-properties/verify-performance.mjs`
to independently reconstruct all intermediate records, including adverse
observations and unchanged fixed upstream failures.
