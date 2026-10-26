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

## Lexical goals

`Lexer::new(source)` and `next_token(LexicalGoal)` provide incremental scanning.
The caller chooses a goal for each token based on its syntactic context:

- `LexicalGoal::Div` recognizes `/` and `/=` as punctuators.
- `LexicalGoal::RegExp` recognizes a complete regular-expression literal,
  including delimiters and flags, as `TokenKind::RegularExpression`.

Both goals skip comments before scanning a token. In particular, `//` is a
line comment, not an empty regular expression; use `/(?:)/` for that spelling.
Escaped characters and character classes protect slashes inside a pattern.
Line terminators, including escaped ones, and unterminated patterns or classes
produce errors with byte spans. Error spans cover the offending line terminator
or the opening slash through end of input for unterminated literals.

Flags consume Unicode `ID_Continue` characters plus `$`, ZWNJ, and ZWJ, using
the `unicode-id-start` dependency's Unicode tables. Identifier escapes are not
part of flag scanning. Pattern syntax, supported flags, duplicate flags, and
incompatible flags are left to a future RegExp parser. This follows the split
between lexical recognition and pattern validation in
[ECMAScript regular-expression literals](https://tc39.es/ecma262/multipage/ecmascript-language-lexical-grammar.html#sec-literals-regular-expression-literals).

Tokens retain borrowed spelling, byte spans, and preceding line-terminator
metadata. An error leaves the cursor unchanged, allowing a retry with a different
goal. End tokens can be read repeatedly; trailing trivia is consumed by the
first end token only. Subsequent end tokens have the same empty span and a false
line-terminator flag.

The existing `tokenize(source)` API preserves its previous behavior and still
rejects slash outside comments. It does not guess goals from preceding tokens.
Callers needing division or regular expressions should migrate to `Lexer` and
select the goal for every `next_token` call. Template-tail goals are deferred to
template-literal support.

## Remaining limitations

This is a bootstrap subset, not a conforming ECMAScript lexer. Unicode
identifiers, identifier escapes, string escapes, non-decimal numbers, numeric
separators, BigInt, templates, hashbangs, and optional chaining are not supported.
Unsupported sequences can sometimes be
split into individually valid tokens; tokenization does not validate grammar.
A future parser must use the line-terminator metadata to implement restricted
productions and automatic semicolon insertion; tokenization does not insert
semicolons.

Next steps include extending literal support and adding an expression AST and
parser. Execution, garbage collection, and stable embedding
APIs remain later milestones.
