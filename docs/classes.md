# Class execution

The public Engine executes base and derived class definitions through the shared
AST class evaluator. Class declarations and top-level class expressions select
AST before Script execution. Within eligible function bodies, a verified
`CreateClass` instruction invokes that same evaluator with the current lexical
environment, so surrounding supported loops can execute as bytecode. This also
works through explicit Bytecode when the complete Script is supported; it does
not lower class elements or constructors to new bytecode instructions.

Anonymous class variable initializers and identifier assignments preserve name
inference, including parenthesized expressions. Anonymous class object-property
values and logical assignments select AST before execution. Classes keep scalar binding elision disabled because methods can capture
bindings. Loop-header reuse requires a conservative full-class check that no
header binding or eval is observed; body-local bindings remain distinct. The bridge resolves enclosing this and new.target without
reading an uninitialized this binding until class source actually requires it.

Within an Auto AST Script, eligible simple-parameter ordinary and class function
bodies independently use the bytecode function runtime. Explicit Ast keeps
function bodies in the AST interpreter. Unsupported bodies select AST before
execution; runtime errors never trigger a second execution.

The per-evaluation cache shares immutable code, not class environments, `this`
or objects. Span/kind lookup is checked against the complete function AST, so
unrelated definitions cannot reuse code by coincidental source offsets. Code
holds only weak cache links; class instances and captured names retain the
normal traced runtime ownership.

Anonymous classes capture the enclosing environment directly: they introduce
no class-name binding and need no empty declarative environment. Named classes
keep their own immutable binding and its temporal dead zone. Both forms still
evaluate heritage and computed keys in strict mode with the enclosing `this`
and new target. Methods retain traced access to captured outer bindings even
when garbage collection runs during construction or method execution.

Class functions initialize their complete `length` and `name` descriptors
together while the ordinary function object is still fresh. This reuses the
existing internal function-property initializer rather than validating and
reviving descriptors for independent updates. Descriptor flags and own-key
order remain unchanged, including inferred names, symbol method names,
accessor prefixes and the length cut-off at default/rest parameters.

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

Super-property get/set/call in object and class methods execute through Ast
and Auto; see [super references](super-properties.md) for scope and ordering.
[Public instance fields](instance-fields.md) initialize through Ast and Auto.
Static initialization (#158) and private elements (#159/#160) remain separate work.
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
traced through stored closures and suspended evaluators. General derived bodies use
the AST evaluator in both Ast and Auto; the bytecode leaf/activation-elision paths
do not model uninitialized constructor `this`. Private elements and complete
builtin subclassing remain separate work.

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

A constructor whose entire body is `super(...)`, with only simple parameters
and direct parameter references as arguments, uses a cached delegation plan.
It preserves argument order, repetition, missing undefined values and omission
of extra actual arguments. A matching argument prefix can be borrowed directly.
This shape cannot observe or capture its constructor activation, so it needs no
managed function environment. Parent lookup and construction still occur on
each call with the original new target. Defaults, spread, eval, arguments,
closures, this accesses and additional statements retain general evaluation;
runtime errors never retry another path. Delegated construction initializes its
own instance fields after the parent returns, before returning the instance.

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

## Retained derived qualification

The fixed derived suite contains five source workloads, with unchanged counts
and result oracles. The initial implementation, arguments allocation change,
and native-prototype candidate each retain independent raw measurements and
verifiers under [benchmarks/derived-classes](../benchmarks/derived-classes).
The arguments candidate has fifteen significant Boa wins; the subsequent native
prototype candidate has thirteen wins and two comparisons within noise. Its
identical-binary control also has one apparent slowdown, which remains in the
archive. These observations require further host qualification before a final
performance claim.

The native-prototype candidate executes all 401 variants of 203 fixed upstream
original files in each of Ast and Auto: 214 pass, 187 fail, zero skipped or timed
out. All 208 preceding passes remain passing. The separate base-class selection
has 276 passes and 18 dependency failures; the 216-variant PR gate passes fully.
Raw failures remain available for dependency review; they do not count as passes.
The [per-variant dependency audit](../benchmarks/derived-classes/dependencies-v1/audit.json)
accounts for all 187 failures in 96 files and links their observed blockers to
separate issues. Twenty-two class-free probes, executed in both Ast and Auto,
confirm missing native APIs and generic iterator behavior. The audit preserves
the original failure signatures and explicitly leaves later assertions unproven.
`node benchmarks/derived-classes/verify-dependencies.mjs` checks it against the
retained upstream sources and both raw captures; it never converts failures to
passes.

The later sole-super candidate (`8b01973`) retains four native derived reports:
candidate/Boa, candidate/before, identical-candidate control and baseline/Boa.
Explicit construction takes 24–29% less time than the address-width candidate
in all three significant before/after comparisons. The Boa comparison has
thirteen significant wins and two noisy comparisons; the identical-worker
control has fifteen noisy comparisons. These are observations, not a universal
speed claim. `verify-delegation.mjs` replays them with the 767 passing native
tests and fixed original captures. The i686 record is a type check, not a
32-bit execution result.

The same sole-super revision also retains 24 hosted reports in
`hosted-delegation-v1`, covering the original eleven, base-class four and
derived five workloads on two machines with all four controls. Derived/Boa
has thirteen significant wins and two noisy comparisons on one machine and
fifteen wins on the other. Both noisy comparisons are class definition; only
one of its three pairs wins significantly on the first machine, so this
revision does not qualify that workload there. Original before/after results
also retain nine significant slowdowns on that machine, alongside one apparent
slowdown in its identical-worker control. `verify-hosted.mjs` checks every
report without converting successful integrity checks into speed acceptance.

The anonymous-environment candidate (`13a91d1`) retains four native derived
reports in `anonymous-env-v1`. All 120 runs and 1,920 output values validate;
the Boa comparison has five significant wins and ten noisy comparisons.
Before/after and identical-candidate controls each have fifteen noisy
comparisons; baseline/Boa has four wins and eleven noisy comparisons. These
observations do not qualify performance. `verify-anonymous-env.mjs` replays
the reports, the 769 passing native tests, all fixed upstream captures and
the unchanged per-variant outcomes and failure signatures against `8b01973`.
The i686 record establishes type checking only. Hosted qualification remains
separate from these native observations.

The two-host anonymous-environment capture retains all 24 reports in
`hosted-anonymous-env-v1`. Both hosts have 33 original-suite Boa wins and
12 base-class wins. Derived/Boa has 13 wins and two noisy comparisons on
EPYC 9V74, and 14 wins and one noisy comparison on EPYC 9V45. Class definition
has only one significant win in three pairs on 9V74, so this candidate still
does not qualify that workload there. The original identical-worker control
retains one apparent slowdown on 9V74, and the derived identical-worker control
retains one apparent speedup on 9V45. Every before/after comparison is noisy.

Five complete upstream captures (`336a38d`, `2be3d7c`, `8b01973`, `13a91d1`, `43d3ff7`) retain all
100,211 variants each: 26,276 pass, 64,978 fail, 18 skip and 8,939 time out,
with zero missing or unexecuted variants. `verify-full.mjs` independently
reconstructs all eight known-pass gates for each source. Passing those gates
does not turn the recorded failures or timeouts into conformance passes.

The fresh-function metadata candidate (`43d3ff7`) retains four native derived
reports in `function-metadata-v1`. Candidate/Boa and baseline/Boa each have
fifteen significant wins. Before/after and identical-candidate controls each
have fifteen noisy comparisons, so these records do not establish a significant
effect of the optimization itself. All 120 runs and 1,920 output values validate.
`verify-function-metadata.mjs` replays those records, 770 passing native tests,
the fixed upstream captures and unchanged failure signatures. It also checks
the actual source13a baseline identity; a rejected packaging archive with an
incorrect copied identity remains retained and never counts as qualification.
Current-source full-corpus, security and two-host qualification remain separate.

Source `43d3ff7` also retains all 24 two-host reports in
`hosted-function-metadata-v1`. Original/Boa has 33 significant wins and
base-class/Boa has 12 on each host. Derived/Boa has 13 wins and two noisy
comparisons on EPYC 9V45 and 15 wins on EPYC 7763. Definition wins only one
of three pairs on 9V45, so the two-host performance criterion remains unmet.
Both hosts' identical-candidate controls are entirely noisy. Before/after
retains one original-suite and one base-class speedup on 7763, with every
other comparison noisy. The full source43d capture independently preserves
all eight known-pass gates; neither integrity replay nor those gates waive
the unresolved performance condition.

The historical two-host prototype/address-width measurements retain all twelve
reports per machine, including slower before/after decisions and apparent
speed changes between identical binaries. `verify-hosted.mjs` checks every
original-eleven, base-four and derived-five report and control. The separate
`verify-view-index.mjs` keeps native controls, the interrupted earlier run,
the ARM byte-identity record and the rejected x64 identity transfer.

The function class-evaluator bridge at `306c32c` retains its local native and
controlled-performance results in
`benchmarks/derived-classes/class-bridge-v1`. Native release tests pass776 with
26 ignored. Fixed upstream results and all187 failed signatures per backend
match the preceding runtime. Local derived/Boa has14 significant wins and1
noisy comparison; before/after has1 improvement and14 noisy comparisons. All
120 worker runs and1920 checked values, including the identical-worker and
baseline/Boa controls, remain available to independent replay. The reference
worker was restored after build-directory cleanup and exactly matches its
historical SHA-256; the rejected premeasurement attempt is also retained.

The same runtime's complete Test262 run41083660571 is retained in
`benchmarks/derived-classes/full-class-bridge-v1`. All eight known-pass gates
were independently replayed. Its100211 outcomes all match43d3ff7:
26276 pass,64978 fail,18 skip and8939 timeout, with no missing variants.
Those failures and timeouts remain unqualified; the result establishes no
whole-corpus conformance claim. Two-host performance and security acceptance
remain separate requirements.

The completed two-host run41083660459 retains all24 reports in
`benchmarks/derived-classes/hosted-class-bridge-v1`. Each derived workload has
at least two significant Boa wins on each machine:14 wins/1 noisy comparison
on EPYC9V74 and15 wins on EPYC7763. Original-eleven/Boa has33 wins on each.
Base-class/Boa has11 wins/1 noisy on9V74 and9 wins/3 noisy on7763; base
construction has only1/3 significant wins on7763, despite lower medians in
all three pairs. Therefore preservation of the existing base-class performance
is not fully demonstrated by this capture. Do not turn those noisy comparisons
into acceptance or rerun unchanged source to select favorable machines.

The9V74 before/after control also records an array slowdown of10–13% in all
three pairs; its actual Boa comparisons remain significant wins. The7763
before/after original comparisons are all within noise, and all identical-worker
controls are within noise on both machines. Every result is retained. The
new derived-workload criterion is met, but this is not unconditional performance
qualification or a reason to omit the base-class and array observations.

Direct construction of a base class can reuse its traced instance-prototype
object: the class evaluator defines the constructor's own prototype as that
same object, non-writable and non-configurable, before publishing it. This path
requires the actual new target to be the class constructor itself. Ordinary
functions and alternate new targets, including Proxy wrappers and subclasses,
retain GetPrototypeFromConstructor and all observable property access. Bound
construction follows its normal new-target substitution before this check.
Changing the instance prototype's parent remains observable by new instances.

A verified class loop can reuse its header cells only when the complete class
syntax does not mention any cloned loop binding or eval. The scan includes
heritage, computed keys, parameter defaults and nested functions/classes;
shadowed name occurrences conservatively retain distinct cells. Public instance
field initializers and nested defaults/arrows are included; static/private
elements keep the previous path. Body-local bindings and fresh class objects
retain their separate identities. References or pending calls spanning a clone
continue to prohibit reuse.

Runtime-created function frames keep their actual argument and parameter
bindings whenever those can be observed. That requirement disables scalar
binding elision independently of the verified loop-cell proof. Internal calls
can reuse unobserved loop headers while retaining captured arguments and
body-local records. The public caller-prepared frame API remains fully backed
by its Environment and keeps its previous execution path.

Runtime-created function frames apply this loop-reuse proof independently of
scalar-local elision. An observable arguments object still requires physical
argument/parameter bindings; it does not by itself expose an unrelated loop
header. The public VM entry for caller-prepared frames retains its existing
binding publication behavior. Scalar-local eligibility keeps its separate
unobservability and numeric-argument checks.

Source a10e7a3 retains its ARM64 measurements in
`benchmarks/derived-classes/class-loop-runtime-v1`. Run
`node benchmarks/derived-classes/verify-class-loop-runtime.mjs` to replay the
216 runs and3456 checked values, source/binary identities, fixed Test262
selection and unchanged failed diagnostics. All12 base-class and15 derived
Boa comparisons are significantly faster. All before/after and identical-worker
comparisons are within noise; no significant before/after gain is established.
Native tests pass779 with26 ignored. Each Ast/Auto fixed upstream selection
has214 passes and187 failures; the base selection has276 passes and18
dependency failures. The PR selection passes216 variants. These local results
do not establish the still-pending two-host, full-corpus and security gates.

Source a10e7a3 also retains all24 reports from two-host run41105090994 in
`hosted-class-loop-runtime-v1`. EPYC9V74 has33 original-eleven Boa wins,
12 base-class wins and14 derived wins/1 noisy comparison. EPYC7763 has33,
12 and15 respectively. Every base/derived workload meets the fixed minimum
of two significant wins in three pairs on each host. Identical-worker controls
are entirely noisy. Before/after retains arithmetic pair0 at11.23% slower on
9V74 and closure-allocation pair2 at10.66% slower on7763; all base/derived
before/after comparisons are noisy. These observations remain recorded.

The same source's full run41105090864 is retained in
`full-class-loop-runtime-v1`. All eight known-pass gates were independently
replayed over100211 variants:26276 pass,64978 fail,18 skip,8939 timeout,
zero missing. This records the remaining failures and establishes no complete
ECMAScript conformance claim. Security and final-head checks remain separate.
