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
Namespace membership tests inspect export names without reading their values;
Get and GetOwnProperty still throw for uninitialized exports.

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

AST references and verified bytecode share the same receiver-aware Reference
operations. LoadSuperBinding captures the initialized receiver and owning
function before a computed key expression; ResolveSuperProperty then reads the
current HomeObject parent. Verification checks function scope and operand flow;
invalid runtime operands throw TypeError. Eligible methods can also create class
expressions through CreateClass metadata, preserving the enclosing environment
for super in heritage and computed keys. Unsupported bodies select their ordinary
execution path before effects. Fields/private elements, eval and general async or
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
surrounding supported loops run in the VM. Initializers prevent scalar binding
elision. Loop-header reuse additionally requires a complete syntax scan to prove
that no initializer, computed key, spread, parameter default or nested closure
observes any cloned header name or eval. Body environments remain distinct.
Definitions use immutable
AST metadata and create fresh runtime objects, with no retry after an error.
Top-level object methods remain outside the whole-Script compiler subset. This
does not complete general class bytecode lowering (#299).

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

Verified function bytecode can resolve super references with two operations.
LoadSuperBinding reads initialized this and the owning function before the key
expression. ResolveSuperProperty reads the current HomeObject parent after that
expression and retains a shared Reference with the original receiver. Key
conversion remains deferred and cached by the Reference; arbitrary stack values
are checked at runtime and missing function scopes are rejected by verification.

Straight-line leaf plans can retain up to eight local bindings without managed
activation environments, preserving parameter writes, lexical TDZ and const
errors. Captured reads/writes resolve actual outer Environment references.
Calls, eval, arguments observers, closure creation, branches and unsupported
operations retain normal frames before any source effects. Lexical super arrows
keep their shared AST path and existing sole-call delegation optimization.

Source ca1f40f retains all120 worker runs and1920 checked values in
`super-leaf-arrow-v1`. Its804 native tests pass with26 ignored; type checking
for i686 is not a32-bit runtime claim. Fixed601 super and401 derived variants
per backend keep every preceding outcome and failed diagnostic unchanged.
Boa comparisons have10 significant wins and5 noisy results: object-call2/3,
class-call3/3, arrow-call3/3, accessor1/3, computed-update1/3. The last two
workloads remain unqualified. The same-binary control retains one apparent
slowdown; earlier `super-leaf-v1` retains all three real before/after arrow
slowdowns that led to restoring the AST path. No unchanged-source host selection
or changed workload/noise criterion is used.

Leaf super receiver/function operands can use the caller's existing roots only
when an abstract stack check proves those operands feed their matching super
reference operation. Ordinary operations, stores or returns that expose either
operand retain the full VM path. Immediate reference-read/discard and update
sequences reuse the shared Reference directly. The retained d606937 native
records in `super-operands-v1` show no significant before/after improvement
(all15 comparisons noisy); their successful replay is not speed qualification.

Stored compiled calls load their outer Environment only when a numeric capture,
leaf capture or fallback needs it. Complete leaf plans record capture needs before
executing effects; property-only methods retain the traced stored edge without
reviving a call-local facade. The root-activity regression shows100 method calls
avoid100 temporary registrations/removals, preserve their answers and collect to
zero retained objects. Capturing bodies and ordinary frames keep their required
roots, defining Realm, this normalization and exception propagation.

Source211698b retains all120 runs/1920 checked values in `lazy-leaf-v1`.
Object/class/arrow/computed-update workloads meet the native minimum2/3 Boa wins;
accessor remains0/3. Before/after has15 noisy comparisons, so a significant timing
gain is not demonstrated. The same-binary control retains one apparent slowdown.

Ordinary own accessor operations revive only the invoked getter or setter,
without materializing the other method or a complete descriptor. The selected
method is rooted before releasing the property-state borrow. Descriptor APIs
keep their full records; Proxy, namespace, virtual/exotic and legacy-function
properties retain their normal paths. Ordinary data reads use one checked
lookup, and own writable data writes retain their existing shortcut.

Source96901d5 passes808 native tests. The root regression measures400 to200
registrations/removals for100 Get plus100 Set operations and verifies collection
to zero retained objects. Its four retained reports in `accessor-slot-v1` have
nine Boa wins/six noisy comparisons; all15 before/after comparisons are noisy,
so no significant timing improvement is demonstrated. Accessor remains0/3 and
computed-update1/3 in this snapshot. The separate Callgrind archive is diagnostic
instruction-count evidence, including startup/setup and two validated invocations;
it is neither wall-clock timing nor performance acceptance. It identifies leaf
execution, Value drop/clone and active-object caching as substantial instruction
costs to investigate next.

The follow-up branch integrates the actually merged main70cc260 at55907d5.
The merge retains main's class-capture loop proof and independent scalar guard,
while object initializers and super-reference instructions conservatively prevent
loop-header reuse. Its correctness-only `main155-v1` snapshot passes810 native
tests, Clippy and i686 type checking. Every601 super and401 derived outcome and
failed diagnostic per backend matches96901d5; base and PR gates also pass. No
performance result is transferred to this new source without a fresh measurement.

Source5fc8f42 adds that object-initializer capture proof. The allocation regression
compares calls with three and thirty iterations: before the change the longer
call creates27 additional managed objects; after it both calls allocate equally.
Nested default/arrow/spread captures and per-iteration body bindings retain their
separate values. All812 native tests pass, and the fixed601 super/401 derived
outcomes and failed diagnostics remain unchanged per backend.

The retained local ARM64 object-capture-v1 reports have14 significant Boa wins
and one noisy comparison, with at least two wins per workload. Before/after
has11 improvements and four noisy comparisons; identical-binary control has
one apparent slowdown and14 noisy comparisons. All120 runs and1920 values are
independently replayed. Independent hosted qualification, preserved original/base/
derived workloads, full Test262 and final CI/security checks remain pending.

At runtime source299e8d8, all15 native super comparisons are significant Boa
wins. Independent EPYC9V74 and Xeon8573C runners each retain15 super wins and
33 original-eleven wins; base results are11wins/1noise and12wins, derived15wins
on both. Every workload meets the unchanged minimum of two wins in three pairs.
All source before/after comparisons are noisy. Identical-binary controls retain
one native apparent speedup/slowdown and one18.60% hosted prop-mega slowdown;
these observations are preserved rather than treated as source regressions.

The new full run41130396942 independently replays all100211 variants with
26397pass,64857fail,18skip and8939timeout, no missing results and no prior-pass
regressions. Earlier source2a36d63 retains its one namespace regression and is
not substituted for this result. The qualification record and replay tools live
in benchmarks/super-properties; security completion and final-head checks remain
separate merge requirements. These records do not establish full conformance
or general engine superiority.
