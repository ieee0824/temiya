//! Initial, deliberately limited JavaScript tokenization.

/// A UTF-8 byte range in the original source text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// A token borrows its spelling from the source; literals are not decoded yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token<'a> {
    pub kind: TokenKind,
    pub text: &'a str,
    pub span: Span,
    /// Whether preceding whitespace or comments contain a line terminator.
    /// For the first token this covers leading trivia; for `End`, trailing
    /// trivia. Empty input has no preceding line terminator.
    pub has_line_terminator_before: bool,
}

impl<'a> Token<'a> {
    /// Decodes a string literal to UTF-16 code units, preserving lone surrogates.
    ///
    /// Returns `None` for non-string or malformed manually constructed tokens.
    /// Legacy escapes are accepted and marked by [`TokenKind::LegacyString`].
    pub fn string_value(&self) -> Option<Vec<u16>> {
        if !matches!(self.kind, TokenKind::String | TokenKind::LegacyString) {
            return None;
        }
        let (end, value, _) = scan_string(self.text, 0).ok()?;
        (end == self.text.len()).then_some(value)
    }

    /// Returns the decoded IdentifierName for identifier and keyword tokens.
    ///
    /// Unescaped names borrow their spelling; escaped names are decoded without
    /// Unicode normalization. Returns `None` for other kinds or malformed
    /// manually constructed tokens. Escaped keywords remain distinguishable
    /// through [`TokenKind::EscapedKeyword`].
    pub fn identifier_name(&self) -> Option<std::borrow::Cow<'a, str>> {
        if !matches!(
            self.kind,
            TokenKind::Identifier | TokenKind::Keyword | TokenKind::EscapedKeyword
        ) {
            return None;
        }
        let (end, name) = scan_identifier(self.text, 0).ok()?;
        (end == self.text.len()).then_some(name)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Identifier,
    Number,
    /// An integer literal with the BigInt suffix.
    BigInt,
    /// A legacy leading-zero octal integer; forbidden in strict code.
    LegacyOctalNumber,
    /// A legacy leading-zero decimal literal; forbidden in strict code.
    LegacyDecimalNumber,
    String,
    /// A string containing legacy octal or non-octal decimal escapes.
    /// A parser must reject this token in strict code.
    LegacyString,
    /// A regular-expression literal, including delimiters and flags.
    RegularExpression,
    Keyword,
    /// An escaped IdentifierName whose decoded spelling is always reserved.
    /// A parser may accept it as a property name but not as a binding name.
    EscapedKeyword,
    Punctuator,
    End,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexError {
    pub span: Span,
    pub message: &'static str,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for LexError {}

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn string_error(start: usize, end: usize, message: &'static str) -> LexError {
    LexError {
        span: Span { start, end },
        message,
    }
}

fn push_code_point(value: &mut Vec<u16>, code: u32) {
    if code <= 0xffff {
        value.push(code as u16);
    } else {
        value.push(0xd800 + ((code - 0x10000) >> 10) as u16);
        value.push(0xdc00 + ((code - 0x10000) & 0x3ff) as u16);
    }
}

fn scan_string(source: &str, start: usize) -> Result<(usize, Vec<u16>, bool), LexError> {
    let Some(quote @ ('\'' | '"')) = source[start..].chars().next() else {
        return Err(string_error(start, start, "expected string literal"));
    };
    let mut pos = start + 1;
    let mut value = Vec::new();
    let mut legacy = false;
    while let Some(c) = source[pos..].chars().next() {
        if c == quote {
            return Ok((pos + 1, value, legacy));
        }
        if matches!(c, '\r' | '\n') {
            return Err(string_error(pos, pos + 1, "unescaped newline in string"));
        }
        if c == '\\' {
            let (end, cooked, old_escape) = scan_string_escape(source, pos)?;
            pos = end;
            value.extend(cooked);
            legacy |= old_escape;
        } else {
            push_code_point(&mut value, c as u32);
            pos += c.len_utf8();
        }
    }
    Err(string_error(start, pos, "unterminated string"))
}

fn scan_string_escape(source: &str, start: usize) -> Result<(usize, Vec<u16>, bool), LexError> {
    let mut pos = start + 1;
    let Some(c) = source[pos..].chars().next() else {
        return Err(string_error(start, pos, "unterminated string escape"));
    };
    pos += c.len_utf8();
    if is_line_terminator(c) {
        if c == '\r' && source.as_bytes().get(pos) == Some(&b'\n') {
            pos += 1;
        }
        return Ok((pos, Vec::new(), false));
    }
    let mut legacy = false;
    let code = match c {
        'b' => 8,
        'f' => 12,
        'n' => 10,
        'r' => 13,
        't' => 9,
        'v' => 11,
        'x' => {
            let mut code = 0;
            for _ in 0..2 {
                let digit = source
                    .as_bytes()
                    .get(pos)
                    .and_then(|b| (*b as char).to_digit(16))
                    .ok_or_else(|| string_error(start, pos, "invalid hexadecimal string escape"))?;
                code = code * 16 + digit;
                pos += 1;
            }
            code
        }
        'u' => {
            let (end, code) = unicode_escape(source, start)?;
            pos = end;
            code
        }
        '0'..='7' => {
            legacy = c != '0' || source.as_bytes().get(pos).is_some_and(u8::is_ascii_digit);
            let mut code = c as u32 - '0' as u32;
            let limit = if c <= '3' { 2 } else { 1 };
            for _ in 0..limit {
                let Some(b @ b'0'..=b'7') = source.as_bytes().get(pos) else {
                    break;
                };
                code = code * 8 + (b - b'0') as u32;
                pos += 1;
            }
            code
        }
        '8' | '9' => {
            legacy = true;
            c as u32
        }
        _ => c as u32,
    };
    let mut value = Vec::new();
    push_code_point(&mut value, code);
    Ok((pos, value, legacy))
}

fn numeric_error(start: usize, end: usize) -> LexError {
    LexError {
        span: Span { start, end },
        message: "invalid numeric literal",
    }
}

fn scan_digits(
    source: &str,
    mut pos: usize,
    radix: u32,
    separators: bool,
) -> Result<usize, LexError> {
    let start = pos;
    let bytes = source.as_bytes();
    let digit = |b: u8| (b as char).is_digit(radix);
    while let Some(&b) = bytes.get(pos) {
        if digit(b) {
            pos += 1;
        } else if b == b'_' {
            if !separators || pos == start || !bytes.get(pos + 1).is_some_and(|b| digit(*b)) {
                return Err(numeric_error(pos, pos + 1));
            }
            pos += 1;
        } else {
            break;
        }
    }
    Ok(pos)
}

fn scan_number(source: &str, start: usize) -> Result<(usize, TokenKind), LexError> {
    let bytes = source.as_bytes();
    let mut pos = start;
    let mut kind = TokenKind::Number;
    let mut integer = true;
    let radix = if bytes.get(start) == Some(&b'0') {
        match bytes.get(start + 1) {
            Some(b'b' | b'B') => 2,
            Some(b'o' | b'O') => 8,
            Some(b'x' | b'X') => 16,
            _ => 10,
        }
    } else {
        10
    };
    if radix != 10 {
        pos = scan_digits(source, start + 2, radix, true)?;
        if pos == start + 2 {
            return Err(numeric_error(start, pos));
        }
    } else {
        if bytes[pos] != b'.' {
            pos = scan_digits(source, pos, 10, true)?;
            if bytes[start] == b'0' && pos > start + 1 {
                if source[start..pos].contains('_') {
                    return Err(numeric_error(start, pos));
                }
                kind = if bytes[start..pos].iter().all(|b| *b <= b'7') {
                    TokenKind::LegacyOctalNumber
                } else {
                    TokenKind::LegacyDecimalNumber
                };
            }
        }
        if kind != TokenKind::LegacyOctalNumber {
            if bytes.get(pos) == Some(&b'.') {
                integer = false;
                pos = scan_digits(source, pos + 1, 10, kind == TokenKind::Number)?;
            }
            if matches!(bytes.get(pos), Some(b'e' | b'E')) {
                integer = false;
                pos += 1;
                if matches!(bytes.get(pos), Some(b'+' | b'-')) {
                    pos += 1;
                }
                let digits = pos;
                pos = scan_digits(source, pos, 10, kind == TokenKind::Number)?;
                if digits == pos {
                    return Err(numeric_error(start, pos));
                }
            }
        }
    }
    if bytes.get(pos) == Some(&b'n') && integer && kind == TokenKind::Number {
        pos += 1;
        kind = TokenKind::BigInt;
    }
    if let Some(c) = source[pos..].chars().next() {
        if is_identifier_start(c) || c.is_ascii_digit() || c == '\\' {
            return Err(numeric_error(start, pos + c.len_utf8()));
        }
    }
    Ok((pos, kind))
}

fn is_identifier_start(c: char) -> bool {
    unicode_id_start::is_id_start(c) || matches!(c, '$' | '_')
}

fn is_identifier_part(c: char) -> bool {
    unicode_id_start::is_id_continue(c) || matches!(c, '$' | '\u{200c}' | '\u{200d}')
}

fn unicode_escape(source: &str, start: usize) -> Result<(usize, u32), LexError> {
    let mut pos = start + 1;
    let invalid = |end| LexError {
        span: Span { start, end },
        message: "invalid Unicode escape",
    };
    if source.as_bytes().get(pos) != Some(&b'u') {
        return Err(invalid(pos));
    }
    pos += 1;
    let braced = source.as_bytes().get(pos) == Some(&b'{');
    if braced {
        pos += 1;
    }
    let digits = pos;
    let mut value = 0u32;
    while let Some(c) = source[pos..].chars().next() {
        let Some(digit) = c.to_digit(16) else {
            break;
        };
        value = value
            .checked_mul(16)
            .and_then(|v| v.checked_add(digit))
            .filter(|v| *v <= 0x10ffff)
            .ok_or_else(|| invalid(pos + c.len_utf8()))?;
        pos += c.len_utf8();
        if !braced && pos - digits == 4 {
            break;
        }
    }
    if braced {
        if pos == digits || source.as_bytes().get(pos) != Some(&b'}') {
            return Err(invalid(pos));
        }
        pos += 1;
    } else if pos - digits != 4 {
        return Err(invalid(pos));
    }
    Ok((pos, value))
}

fn scan_identifier(
    source: &str,
    start: usize,
) -> Result<(usize, std::borrow::Cow<'_, str>), LexError> {
    let mut pos = start;
    let mut decoded: Option<String> = None;
    while let Some(c) = source[pos..].chars().next() {
        let escaped = c == '\\';
        let (end, c) = if escaped {
            let (end, code) = unicode_escape(source, pos)?;
            let c = char::from_u32(code).ok_or(LexError {
                span: Span { start: pos, end },
                message: "invalid character in identifier",
            })?;
            (end, c)
        } else {
            (pos + c.len_utf8(), c)
        };
        let valid = if pos == start {
            is_identifier_start(c)
        } else {
            is_identifier_part(c)
        };
        if !valid {
            if escaped || pos == start {
                return Err(LexError {
                    span: Span { start: pos, end },
                    message: "invalid character in identifier",
                });
            }
            break;
        }
        if escaped {
            decoded.get_or_insert_with(|| source[start..pos].to_owned());
        }
        if let Some(name) = &mut decoded {
            name.push(c);
        }
        pos = end;
    }
    if pos == start {
        return Err(LexError {
            span: Span { start, end: pos },
            message: "expected identifier",
        });
    }
    Ok((
        pos,
        decoded
            .map(std::borrow::Cow::Owned)
            .unwrap_or_else(|| std::borrow::Cow::Borrowed(&source[start..pos])),
    ))
}

fn scan_regexp(source: &str, start: usize) -> Result<usize, LexError> {
    let mut pos = start + 1;
    let mut in_class = false;
    let mut escaped = false;
    while let Some(c) = source[pos..].chars().next() {
        if is_line_terminator(c) {
            return Err(LexError {
                span: Span {
                    start: pos,
                    end: pos + c.len_utf8(),
                },
                message: "line terminator in regular-expression literal",
            });
        }
        pos += c.len_utf8();
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '[' if !in_class => in_class = true,
            ']' if in_class => in_class = false,
            '/' if !in_class => {
                while let Some(c) = source[pos..].chars().next() {
                    if !is_identifier_part(c) {
                        break;
                    }
                    pos += c.len_utf8();
                }
                return Ok(pos);
            }
            _ => {}
        }
    }
    Err(LexError {
        span: Span { start, end: pos },
        message: if in_class {
            "unterminated regular-expression character class"
        } else {
            "unterminated regular-expression literal"
        },
    })
}

/// Selects how a slash outside comments is interpreted for the next token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LexicalGoal {
    /// Recognize division and division assignment (InputElementDiv subset).
    Div,
    /// Recognize a regular-expression literal (InputElementRegExp subset).
    RegExp,
}

/// An incremental lexer whose caller selects a lexical goal for every token.
///
/// Tokens borrow the source. An error leaves the cursor unchanged, so the
/// caller may retry with another goal. Repeated end tokens have empty spans;
/// only the first end token can carry trailing line-terminator metadata.
///
/// ```
/// use temiya::lexer::{Lexer, LexicalGoal, TokenKind};
/// let mut lexer = Lexer::new("/a/ / 2");
/// assert_eq!(lexer.next_token(LexicalGoal::RegExp)?.kind, TokenKind::RegularExpression);
/// assert_eq!(lexer.next_token(LexicalGoal::Div)?.text, "/");
/// assert_eq!(lexer.next_token(LexicalGoal::Div)?.text, "2");
/// # Ok::<(), temiya::lexer::LexError>(())
/// ```
#[derive(Debug)]
pub struct Lexer<'a> {
    source: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    /// Creates a lexer at the beginning of the source.
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        Self { source, pos: 0 }
    }

    /// Reads the next token using the supplied goal after skipping comments.
    ///
    /// Pattern syntax and flag validity are deferred to a future RegExp parser.
    pub fn next_token(&mut self, goal: LexicalGoal) -> Result<Token<'a>, LexError> {
        self.read(Some(goal))
    }

    fn read(&mut self, goal: Option<LexicalGoal>) -> Result<Token<'a>, LexError> {
        let token = scan_token(self.source, self.pos, goal)?;
        self.pos = token.span.end;
        Ok(token)
    }
}

/// Tokenizes the bootstrap subset described in `docs/lexer.md`.
///
/// This compatibility API still rejects slash outside comments. Use [`Lexer`]
/// to select a division or regular-expression goal for each token.
pub fn tokenize(source: &str) -> Result<Vec<Token<'_>>, LexError> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.read(None)?;
        let end = token.kind == TokenKind::End;
        tokens.push(token);
        if end {
            return Ok(tokens);
        }
    }
}

fn scan_token(
    source: &str,
    mut pos: usize,
    goal: Option<LexicalGoal>,
) -> Result<Token<'_>, LexError> {
    let bytes = source.as_bytes();
    let mut has_line_terminator_before = false;
    while pos < bytes.len() {
        let start = pos;
        let c = source[pos..].chars().next().unwrap();
        if matches!(
            c,
            ' ' | '\t'
                | '\u{000b}'
                | '\u{000c}'
                | '\n'
                | '\r'
                | '\u{00a0}'
                | '\u{feff}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            has_line_terminator_before |= is_line_terminator(c);
            pos += c.len_utf8();
            continue;
        }
        if source[pos..].starts_with("//") {
            pos += 2;
            while pos < bytes.len() {
                let c = source[pos..].chars().next().unwrap();
                if is_line_terminator(c) {
                    break;
                }
                pos += c.len_utf8();
            }
            continue;
        }
        if source[pos..].starts_with("/*") {
            let Some(end) = source[pos + 2..].find("*/") else {
                return Err(LexError {
                    span: Span {
                        start,
                        end: bytes.len(),
                    },
                    message: "unterminated block comment",
                });
            };
            has_line_terminator_before |= source[pos + 2..pos + 2 + end]
                .chars()
                .any(is_line_terminator);
            pos += end + 4;
            continue;
        }
        let kind = if c == '/' && goal.is_some() {
            if goal == Some(LexicalGoal::Div) {
                pos += if source[pos..].starts_with("/=") {
                    2
                } else {
                    1
                };
                TokenKind::Punctuator
            } else {
                pos = scan_regexp(source, pos)?;
                TokenKind::RegularExpression
            }
        } else if is_identifier_start(c) || c == '\\' {
            let (end, name) = scan_identifier(source, pos)?;
            pos = end;
            if is_keyword(&name) {
                if matches!(name, std::borrow::Cow::Owned(_)) {
                    TokenKind::EscapedKeyword
                } else {
                    TokenKind::Keyword
                }
            } else {
                TokenKind::Identifier
            }
        } else if c.is_ascii_digit()
            || (c == '.' && bytes.get(pos + 1).is_some_and(u8::is_ascii_digit))
        {
            let (end, kind) = scan_number(source, pos)?;
            pos = end;
            kind
        } else if matches!(c, '\'' | '"') {
            let (end, _, legacy) = scan_string(source, pos)?;
            pos = end;
            if legacy {
                TokenKind::LegacyString
            } else {
                TokenKind::String
            }
        } else {
            // Longest spelling wins. Slash is handled by the lexical goal above.
            let punct = [
                ">>>=", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "...", "=>",
                "==", "!=", "<=", ">=", "++", "--", "**", "<<", ">>", "&&", "||", "??", "+=", "-=",
                "*=", "%=", "&=", "|=", "^=", "{", "}", "(", ")", "[", "]", ".", ";", ",", "<",
                ">", "+", "-", "*", "%", "&", "|", "^", "!", "~", "?", ":", "=",
            ]
            .into_iter()
            .find(|p| source[pos..].starts_with(p));
            let Some(punct) = punct else {
                return Err(LexError {
                    span: Span {
                        start,
                        end: start + c.len_utf8(),
                    },
                    message: "unsupported character or syntax",
                });
            };
            pos += punct.len();
            TokenKind::Punctuator
        };
        return Ok(Token {
            kind,
            text: &source[start..pos],
            span: Span { start, end: pos },
            has_line_terminator_before,
        });
    }
    Ok(Token {
        kind: TokenKind::End,
        has_line_terminator_before,
        text: "",
        span: Span {
            start: pos,
            end: pos,
        },
    })
}

fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_declaration_with_source_ranges() {
        let source = "const answer = 4.2e+1; // comment\n";
        let tokens = tokenize(source).unwrap();
        assert_eq!(
            tokens.iter().map(|t| (t.kind, t.text)).collect::<Vec<_>>(),
            vec![
                (TokenKind::Keyword, "const"),
                (TokenKind::Identifier, "answer"),
                (TokenKind::Punctuator, "="),
                (TokenKind::Number, "4.2e+1"),
                (TokenKind::Punctuator, ";"),
                (TokenKind::End, "")
            ]
        );
        for token in tokens {
            assert_eq!(&source[token.span.start..token.span.end], token.text);
        }
    }

    #[test]
    fn preserves_unicode_string_byte_spans() {
        let tokens = tokenize("/*雪*/ '手宮'").unwrap();
        assert_eq!(tokens[0].text, "'手宮'");
        assert_eq!(tokens[0].span, Span { start: 8, end: 16 });
    }

    #[test]
    fn uses_longest_punctuator() {
        let tokens = tokenize("a >>>= 1; b !== .5").unwrap();
        assert_eq!(tokens[1].text, ">>>=");
        assert_eq!(tokens[5].text, "!==");
        assert_eq!(tokens[6].text, ".5");
    }

    #[test]
    fn reports_incomplete_and_unsupported_input() {
        for source in [
            "/*",
            "'abc",
            "1e+",
            "123abc",
            "'a\nb'",
            "`template`",
            "/regex/",
            "a / b",
        ] {
            assert!(tokenize(source).is_err(), "{source:?}");
        }
    }

    #[test]
    fn comment_ends_at_unicode_line_separator() {
        assert_eq!(tokenize("// comment\u{2028}42").unwrap()[0].text, "42");
        assert_eq!(tokenize("").unwrap()[0].kind, TokenKind::End);
    }

    #[test]
    fn tracks_all_line_terminators_in_whitespace_and_comments() {
        for newline in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
            for source in [
                format!("a{newline}b c"),
                format!("a/*雪{newline}*/b c"),
                format!("a//雪{newline}b c"),
            ] {
                let tokens = tokenize(&source).unwrap();
                assert_eq!(
                    tokens
                        .iter()
                        .map(|t| t.has_line_terminator_before)
                        .collect::<Vec<_>>(),
                    [false, true, false, false],
                    "{source:?}"
                );
                assert_eq!(tokens[1].text, "b");
                for token in tokens {
                    assert_eq!(&source[token.span.start..token.span.end], token.text);
                }
            }
        }
    }

    #[test]
    fn trivia_without_newlines_does_not_set_metadata() {
        for source in [
            "a b",
            "a/**/b",
            "a/*雪*/ /*comment*/b",
            "a\t\u{000b}\u{000c}\u{00a0}\u{feff}b",
        ] {
            assert!(
                tokenize(source)
                    .unwrap()
                    .iter()
                    .all(|t| !t.has_line_terminator_before),
                "{source:?}"
            );
        }
    }

    #[test]
    fn tracks_leading_trailing_and_comment_only_trivia() {
        for (source, expected) in [
            ("", vec![false]),
            ("/**/ //comment", vec![false]),
            ("\n", vec![true]),
            ("/*\n*/ //comment", vec![true]),
            ("\na", vec![true, false]),
            ("a\n", vec![false, true]),
            ("a/*\n*/", vec![false, true]),
            ("a//comment", vec![false, false]),
            ("a//comment\n", vec![false, true]),
            ("a/**//*\n*//**/ b c", vec![false, true, false, false]),
            ("a\n/**/ /*comment*/b", vec![false, true, false]),
        ] {
            let tokens = tokenize(source).unwrap();
            assert_eq!(
                tokens
                    .iter()
                    .map(|t| t.has_line_terminator_before)
                    .collect::<Vec<_>>(),
                expected,
                "{source:?}"
            );
            let end = tokens.last().unwrap();
            assert_eq!(end.kind, TokenKind::End);
            assert_eq!(
                end.span,
                Span {
                    start: source.len(),
                    end: source.len()
                }
            );
        }
    }
}
