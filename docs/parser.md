# Expression parser

`parser::parse_expression` parses one complete expression into an owned
`ast::Expr`, rejecting leftover tokens. `parse_expression_with_options` accepts
Script/Module lexer options, strict mode, and a recursion limit (64 by default).
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

## Statements and source bodies

`parse_script` parses complete Script statements; `parse_program` accepts
explicit options. `parse_function_body` and `parse_async_function_body` parse
function-body fragments without surrounding braces, enabling return and, for
the async entry point, await and for-await-of. Module source is strict and
permits top-level await. Import/export syntax is separate work.

Owned `Program` and `Stmt` nodes cover blocks, empty/expression statements,
if/else, while/do-while, three-clause for, for-in/of/await-of, labels,
break/continue, return, throw, switch, try/catch/finally, debugger, and non-strict
with. Basic var/let/const identifier declarations support loop headers and
statement lists. Lexical declarations cannot occur in single-statement slots.
Destructuring bindings and function/class declarations are added separately.
Every statement, declarator, switch case, and catch clause has a source range;
`Program::validate` recursively checks ranges and containment.

Statement boundaries use explicit semicolons or automatic semicolon insertion
at line terminators, closing braces, EOF, and the end of do-while. Line
terminators after return/break/continue end those restricted forms; throw rejects
a following line terminator. Dangling else binds to the nearest if. Loop and
switch contexts and active label targets are checked. For headers propagate
the NoIn restriction while allowing `in` inside nested primary constructs.
For-in initializer compatibility is limited to non-strict var identifiers;
for-of and for-await-of prohibit declaration initializers.

Source-body directive prologues recognize an unescaped `use strict` spelling.
Legacy escapes in earlier directives are checked retrospectively. A string in
an ordinary nested block does not activate strict mode. Full declaration/scope
early-error validation remains separate work.

This is not yet a complete ECMAScript frontend. Function/class/arrow and module
syntax, binding/destructuring patterns, private
names, `super`, `new.target`, async/yield contexts, method definitions, and full
early-error validation are separate roadmap work. Regexp pattern validation and
runtime evaluation are also separate. The recursion limit bounds nested parser
calls, not arbitrary manually built AST depth or total source length.
