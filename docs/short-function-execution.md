# Short function execution

Internal ordinary-function calls can use a small executor after the existing
empty-function-frame proof succeeds. Constructors, arrows, parameter/arguments
observers and prepared public frames retain their existing entry paths.

Eligibility is computed only after bytecode verification. The body has at most
64 instructions, at most four operand values, no locals or backedges, and only
an explicit supported instruction set. At most one pending name reference is
allowed, and none crosses a branch. Unsupported or future instructions reject
this path before execution. Runtime errors propagate without retrying effects.

The executor keeps owned Values and its pending Reference alive across coercion,
object-environment and metadata callbacks. Popped slots release their roots.
Name reads/writes, updates, coercions, binary operations and object-definition
metadata use the shared runtime operations. Each scope is independently proven
empty; captured outer environments are neither cleared nor reused as locals.
Receiver normalization, function ownership and exception-realm binding remain
in the ordinary function caller. Reentrant GC and foreign Realm tests cover
these boundaries through public Engine Ast/Auto APIs.

Object metadata on this path also omits the search for enclosing `this` and
`new.target` bindings. The same proof rejects those observers, arrows, `super`
and direct `eval` throughout metadata expressions before execution. Metadata
still uses the original environment, Realm and immutable function-code cache;
computed keys and subsequently invoked ordinary methods keep their effects.

This reduces general VM setup for eligible callbacks without changing workload
inputs or statistical gates. Fixed/full corpus, security and two independent
native Boa comparisons remain required for qualification.
