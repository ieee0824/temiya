# Array binding execution

Array binding patterns initialize declarations, ordinary function parameters,
catch parameters, and declarations in supported loops through the shared rooted
IteratorRecord. Names are created before lexical initialization and before
non-simple parameter initialization, preserving TDZ and left-to-right defaults.
Identifier-only declarations and simple parameters retain their existing paths.

Elisions advance the iterator without reading the iterator result's value.
Defaults apply only to undefined and infer anonymous function, arrow and class
names. Nested array patterns recurse; rest drains into a real Array. An unfinished
iterator closes after normal early completion, including an empty pattern, and
on binding/default failures. Original JavaScript throws take precedence over
cleanup throws; next/done/value failures retain the shared record's done state.
No heap-state borrow survives JavaScript callbacks.

Public Engine Ast/Auto coverage is in tests/array_binding.rs, including GC in
iterator callbacks and defaults. Object binding (#163), destructuring assignment,
and complete for-of protocol execution (#171) remain separate work. Existing
for-of iteration is not made conformant by supporting its binding declaration.
Generator/async integration remains subject to those execution paths' limits.

The fixed selection in benchmarks/array-binding contains all destructuring-binding
metadata tests and every /dstr/ path: 8,806 files and 17,414 variants. Cross-feature
failures remain in scope of recording. Full candidate results, toolchain checks
and two-job Boa speed qualification remain pending; no conformance or performance
completion is claimed by the focused integration tests.
