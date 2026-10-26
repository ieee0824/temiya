# Static semantics

Public parser entry points validate their owned AST before returning it.
`semantics::validate_program` and `validate_expression` expose the same checks
for parsed or manually assembled trees. `SemanticOptions` specifies source goal,
strict mode, and function/async/generator fragment context. Errors have source
byte spans and convert to `ParseError` at parser boundaries.

Name queries return decoded names and their binding spans. `bound_names` walks
patterns while excluding default-value expressions. `lexically_declared_names`
and `var_declared_names` take a `DeclarationScope`: direct functions are var
scoped at Script/Function top level and lexical at Module/Block level. Var
collection descends through statements but stops at nested function/class code.
Anonymous default exports use a synthetic `*default*` binding name.

`contains` is a structural feature query over all descendant expression syntax,
including nested functions and binding/property name nodes. It is intentionally
not a scope-boundary filter. The validator uses context-sensitive traversal for
await/yield, arguments, super, and new.target rather than treating that query as
an execution or reference-resolution predicate.

Validation detects duplicate lexical declarations, lexical/var and
parameter/body conflicts, loop lexical/var conflicts, catch binding conflicts,
duplicate exported names, and exports of undeclared local bindings. Var
redeclarations and shadowing in distinct scopes remain allowed. Switch clauses
share one lexical scope. Function boundaries reset labels, loops, and suspension
contexts; arrows preserve lexical super/new.target and arguments restrictions.

Classes introduce private-name scopes, including forward declarations and
references captured by nested functions or classes. Own private declarations do
not extend into heritage expressions. Strict checks cover binding/reference
names, legacy literals, identifier deletion, with, and eval/arguments assignment
targets. Cover-initialized object names are permitted only in binding/assignment
patterns. Rest defaults and trailing commas after rest targets are rejected,
while corresponding spread literals remain valid.

The parser handles grammar-only constraints and preserves metadata needed for
early errors (array trailing commas, property shorthand/separators, function
flags, strict bodies, and source ranges). `Program::validate` with `SourceText`
is the separate structural range check for manually assembled trees. Semantic
validation assumes grammar-shaped nodes and limits recursive checking to 128
levels; it does not prove arbitrary constructed nodes match their original text.
This module does not perform type checking, resolve ordinary identifier
references to environments, or evaluate expressions. Annex B function semantics
and unsupported proposals remain outside the current frontend scope. Test262
coverage is a separate milestone; these checks do not establish conformance.
