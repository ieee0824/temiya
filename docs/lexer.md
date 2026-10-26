# Bootstrap lexer

`temiya::lexer::tokenize` borrows UTF-8 source text and returns tokens with
half-open byte spans, preserving literal spelling. An explicit end token marks
the end of input. Errors also carry byte spans. These APIs are experimental.

Each token exposes `has_line_terminator_before`: whether the whitespace or
comments since the preceding token contain LF, CR, U+2028, or U+2029. CRLF also
sets this boolean. Line comment endings and line terminators inside block
comments count. For the first token, the flag describes leading whitespace and
comments; for the end token, it describes trailing whitespace and comments.
Empty input produces an end token with the flag set to `false`; input containing
only whitespace and comments records any line terminator on the end token.
The flag resets after every token, so a newline affects only the next token.
Token text and byte spans exclude preceding whitespace and comments.

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
A future parser must use the line-terminator metadata to implement restricted
productions and automatic semicolon insertion; tokenization does not insert
semicolons.

Next steps are lexical goal selection, followed by
an expression AST and parser. Execution, garbage collection, and stable embedding
APIs remain later milestones.
