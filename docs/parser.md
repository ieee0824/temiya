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
Destructuring bindings and function/class declarations are also supported as
described below.
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

## Declarations, functions, and classes

Variable declarations and parameters accept nested array/object binding patterns,
defaults, holes, and rest bindings. Rest must be last and cannot have a trailing
comma or default. Function declarations/expressions, arrows, generators, async
functions and async generators retain their parameters, body, flags, and strict
mode in owned AST nodes. Arrow heads use speculative parsing on a cloned parser;
the committed parser advances only when an arrow is recognized. Arrow bodies
inherit lexical super/new.target access, while ordinary functions establish a
new context. Labels and loop/switch targets never cross function boundaries.

Object/class methods, getters and setters enforce arity and unique parameters.
Normal functions allow duplicate simple parameters only in non-strict code;
defaults or patterns make parameters non-simple. Strict directives with non-simple
parameters are rejected. Await/yield are interpreted in their respective function
contexts and rejected in formal parameters. Module source reserves await in
nested code as well. `new.target` is limited to function/initializer contexts.

Classes support heritage, public/private methods, async/generator methods,
constructors, fields, computed keys, and static blocks. Class code is strict.
Constructors cannot be special methods or duplicated; super calls are permitted
only in derived constructors and their arrows. Methods and initializers can
reference super properties, but nested ordinary functions cannot. Private
constructor names, duplicate private definitions (except matching getter/setter
pairs), static prototype names, invalid constructor fields, and private deletion
are rejected. Field initializers and static blocks reject arguments references
and super calls; static blocks also disallow return and await expressions.

See [ECMAScript functions and classes](https://tc39.es/ecma262/multipage/ecmascript-language-functions-and-classes.html).
This is not yet a complete ECMAScript frontend. Module syntax, cover-initialized
object assignment patterns, Annex B function declarations in single-statement
slots, declaration/scope analysis, undeclared private references, and full
early-error validation remain roadmap work. Regexp pattern validation and
runtime evaluation are also separate. The recursion limit bounds nested parser
calls, not arbitrary manually built AST depth or total source length.
