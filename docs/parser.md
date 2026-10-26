# Expression parser

`parser::parse_expression` parses one complete expression into an owned
`ast::Expr`, rejecting leftover tokens. `parse_expression_with_options` accepts
Script/Module lexer options, strict mode, and a recursion limit (128 by default).
Module parsing is strict. Errors include half-open UTF-8 spans, including empty
EOF spans. Source buffers can be dropped after parsing.

The Pratt parser handles primary literals and identifiers, parentheses, arrays
and simple object properties/spreads, unary/update operators, arithmetic,
comparison, bitwise and logical operators, nullish coalescing, conditional,
assignment, and comma expressions. It also handles member access, computed and
optional access, calls, `new`, and tagged/untagged template substitutions.
Parentheses remain explicit in the AST. Exponentiation and assignments associate
to the right; other binary operators associate to the left.

Lexer goals are selected at each token boundary. Primary positions admit
regexps; infix positions admit division; matching template substitution braces
use template-tail goals. A cloned lexer provides lookahead without committing
its cursor. No heuristic is used to guess slash meaning in the lexer.

The parser rejects invalid assignment/update targets, unparenthesized mixtures
of nullish and logical operators, unary expressions before exponentiation,
invalid untagged template escapes, and forbidden optional-chain constructors or
tags. Strict mode rejects legacy numbers/strings, strict reserved names, and
identifier deletion. Full static semantics will be added separately.

This is an expression parser, not yet a complete ECMAScript frontend. Statement,
function/class/arrow and module syntax, binding/destructuring patterns, private
names, `super`, `new.target`, async/yield contexts, method definitions, and full
early-error validation are separate roadmap work. Regexp pattern validation and
runtime evaluation are also separate. The recursion limit bounds nested parser
calls, not arbitrary manually built AST depth or total source length.
