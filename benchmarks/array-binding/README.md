# Array binding implementation inputs (#162)

This branch depends on #161; it is not eligible to merge before #159, #160
and #161. No array binding implementation or speed qualification is claimed.

selection.json freezes all destructuring-binding metadata tests and every /dstr/
path from the complete pinned Test262 catalog before implementation. Object
binding, assignment, async/generator, eval and for-of dependencies remain in the
selection; outcomes must not remove them. selection-freeze.json records corpus,
catalog and per-file hashes. Skips, failures and timeouts are not passes.

Implementation must connect array binding to the shared IteratorRecord in
src/runtime/iterator.rs and cover declarations and function parameters. Required
observable behavior includes:

- BoundNames predeclaration for lexical bindings and non-simple parameters,
  preserving TDZ, left-to-right initialization and earlier parameter visibility.
- Elisions step without reading value; exhausted iterators do not advance again.
- Defaults run only for undefined and retain anonymous function/class naming.
- Rest drains into a real Array with own data properties; nested arrays recurse.
- Early normal completion closes once, including empty patterns. Abrupt defaults
  and nested initialization close unfinished outer iterators with the original
  JavaScript throw taking precedence. Step/done/value failures retain the shared
  record's done behavior. InternalError must never be masked by JavaScript.
- Ast and Auto public Engine tests verify side-effect order, receivers, getter
  failures, missing/non-callable methods, primitive values and reentrant GC.

Existing entry points to update are statement.rs binding_name, declaration
instantiation/evaluation, collect_var_names and applicable catch/for binding
paths, plus function.rs bind_parameters/bind_parameter. Keep identifier paths
fast. Object patterns are #163; assignment patterns are separate work. A selected
cross-feature test is retained even when its other dependency is unfinished.

Freeze performance workloads and validate their actual pinned Boa outputs before
runtime edits or candidate timing. Use original noise/deadline policy and all
prior control workloads; require each case to qualify independently on both
hosted machines. No timing reruns, CPU selection or changed oracle can turn an
adverse observation into a pass. Separate per-worktree Cargo target directories
are mandatory for frozen binaries.

The normative target is ECMAScript2026
[IteratorBindingInitialization](https://tc39.es/ecma262/2026/#sec-runtime-semantics-iteratorbindinginitialization)
and [BindingInitialization](https://tc39.es/ecma262/2026/#sec-runtime-semantics-bindinginitialization).
The frozen selection contains8806 files and17414 variants; none has yet been
claimed as a pass on this branch.
