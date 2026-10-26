# Bootstrap lexer

`temiya::lexer::tokenize` borrows UTF-8 source text and returns tokens with
half-open byte spans, preserving literal spelling. An explicit end token marks
the end of input. Errors also carry byte spans. These APIs are experimental.

This first implementation supports ASCII identifiers, a core keyword set,
decimal numbers (including fractions and exponents), quoted strings without
escapes, common operators and delimiters, line comments, and block comments.
Contextual words such as `let`, `async`, and `await` remain identifiers for a
future parser to interpret.

This is a bootstrap subset, not a conforming ECMAScript lexer. Unicode
identifiers, identifier escapes, string escapes, non-decimal numbers, numeric
separators, BigInt, templates, hashbangs, and optional chaining are not supported.
Slash outside comments is rejected until a parser can select between division
and regular-expression lexical goals. Unsupported sequences can sometimes be
split into individually valid tokens; tokenization does not validate grammar.
A future parser must also account for line terminators and automatic semicolon
insertion; this token stream does not yet retain that information.

Next steps are lexical goal selection and line-terminator metadata, followed by
an expression AST and parser. Execution, garbage collection, and stable embedding
APIs remain later milestones.
