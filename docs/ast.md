# AST ownership and extension

`ast::Expr` owns its children through `Box` and `Vec`, identifier names as Rust
`String`, and string/template values as UTF-16 code units. No node borrows lexer
tokens or source text. A tree may outlive parsing and source buffers without
propagating source lifetimes through static semantics or an interpreter.
Numbers and regexps retain owned raw spellings for later value conversion.
Parentheses, legacy literal categories, and invalid template cooked values are
retained for later grammar and strict-mode checks.

Every expression has a half-open UTF-8 `Span`, including delimiters. Object
properties and template components also have spans. Decoded names and values
are payloads, not separate syntax nodes. Callers keep the source separately for
diagnostics. `Expr::validate(&SourceText)` checks character boundaries, ranges,
child containment, object-property containment, and template component count.
It does not assert that the source matches node spelling or validate grammar.
`visit` traverses syntax in source evaluation order, including conditional
branches unconditionally; it does not execute code.

The AST includes expressions used by the Pratt parser and statements used by
the source-body parser. `ExprKind` and `StmtKind` are non-exhaustive. New syntax
should add an explicit variant with
owned children and spans, extend structural validation and traversal, and add
boundary and error tests. Function, arrow, method, class, class element, binding
pattern, declaration, static import/export, module request, and attribute nodes
retain owned data and ranges; this is not a claim that all ECMAScript syntax is
implemented. Operators retain owned spellings and are validated by the parser.
Shared/interned names, arenas, serialization formats, and stable AST APIs are
future decisions. Syntax ownership remains independent of runtime GC ownership.
