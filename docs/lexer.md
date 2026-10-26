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

This implementation supports Unicode identifiers, a core keyword set,
numeric literals (including fractions and exponents), quoted strings without
escapes, common operators and delimiters, line comments, and block comments.
Contextual words such as `let`, `async`, and `await` remain identifiers for a
future parser to interpret.

## Identifier names

Identifier starts use Unicode `ID_Start` plus `$` and `_`; subsequent characters
use `ID_Continue` plus `$`, ZWNJ, and ZWJ. Unicode tables come from
`unicode-id-start`. Both `\uXXXX` and `\u{...}` escapes are accepted when their
decoded scalar is valid in that position. Surrogates, out-of-range code points,
malformed escapes, and escapes of invalid identifier characters are rejected.
No Unicode normalization is applied.

`Token::identifier_name()` returns the decoded spelling as `Cow<str>` (borrowed
for unescaped names). `Token::text` and byte spans always retain raw spelling.
Unconditionally reserved words are `Keyword`; escaped spellings of those words
are `EscapedKeyword`, so a parser can accept them as property IdentifierNames
while rejecting them as bindings or literal keyword syntax. Contextual and
strict-mode reserved words remain `Identifier`; a parser must check both the
decoded name and whether the raw text contains an escape in the current context.
See the [ECMAScript names and keywords grammar](https://tc39.es/ecma262/multipage/ecmascript-language-lexical-grammar.html#sec-names-and-keywords).

## Numeric literals

`Number` tokens include decimal fractions and exponents, binary (`0b`), octal
(`0o`), and hexadecimal (`0x`) literals, with uppercase prefix variants.
Numeric separators must occur between digits of the same component and radix.
Integer literals ending in `n` produce `BigInt`; fractions, exponents, and
legacy leading-zero forms cannot take that suffix. A numeric literal cannot
directly precede a decimal digit, identifier start, or identifier escape.

Legacy leading-zero integers produce `LegacyOctalNumber` when all digits are
octal, or `LegacyDecimalNumber` when an 8 or 9 occurs. The latter can include a
fraction or exponent. These forms reject separators and are preserved for a
future parser to reject in strict code. A dot following a legacy octal integer
is a separate punctuator, allowing property access. Lexing does not validate
whether adjacent tokens form a valid expression.

All numeric tokens retain source spelling rather than converting to machine
numbers; large literals cannot overflow the scanner. Signs are separate
punctuators. Runtime conversion and arithmetic remain future work. See
[ECMAScript numeric literals](https://tc39.es/ecma262/multipage/ecmascript-language-lexical-grammar.html#sec-literals-numeric-literals).

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

This is a bootstrap subset, not a conforming ECMAScript lexer. Private
identifiers, string escapes, templates, hashbangs, and optional chaining are not supported.
Unsupported sequences can sometimes be
split into individually valid tokens; tokenization does not validate grammar.
A future parser must use the line-terminator metadata to implement restricted
productions and automatic semicolon insertion; tokenization does not insert
semicolons.

Next steps include extending literal support and adding an expression AST and
parser. Execution, garbage collection, and stable embedding
APIs remain later milestones.
