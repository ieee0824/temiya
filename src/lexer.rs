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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Identifier,
    Number,
    String,
    Keyword,
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

/// Tokenizes the bootstrap subset described in `docs/lexer.md`.
///
/// Unsupported input returns an error instead of being silently discarded.
pub fn tokenize(source: &str) -> Result<Vec<Token<'_>>, LexError> {
    let mut tokens = Vec::new();
    let mut pos = 0;
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
        let kind = if c.is_ascii_alphabetic() || matches!(c, '_' | '$') {
            pos += 1;
            while pos < bytes.len()
                && (bytes[pos].is_ascii_alphanumeric() || matches!(bytes[pos], b'_' | b'$'))
            {
                pos += 1;
            }
            if matches!(
                &source[start..pos],
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
            ) {
                TokenKind::Keyword
            } else {
                TokenKind::Identifier
            }
        } else if c.is_ascii_digit()
            || (c == '.' && bytes.get(pos + 1).is_some_and(u8::is_ascii_digit))
        {
            while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
            if bytes.get(pos) == Some(&b'.') {
                pos += 1;
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
            }
            if matches!(bytes.get(pos), Some(b'e' | b'E')) {
                pos += 1;
                if matches!(bytes.get(pos), Some(b'+' | b'-')) {
                    pos += 1;
                }
                let exponent = pos;
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
                if pos == exponent {
                    return Err(LexError {
                        span: Span { start, end: pos },
                        message: "expected exponent digits",
                    });
                }
            }
            if bytes.get(pos).is_some_and(|c| {
                c.is_ascii_alphabetic() || matches!(c, b'_' | b'$' | b'\\') || !c.is_ascii()
            }) {
                return Err(LexError {
                    span: Span { start, end: pos },
                    message: "unsupported or invalid numeric literal",
                });
            }
            TokenKind::Number
        } else if matches!(c, '\'' | '"') {
            let quote = c;
            pos += 1;
            loop {
                let Some(c) = source[pos..].chars().next() else {
                    return Err(LexError {
                        span: Span { start, end: pos },
                        message: "unterminated string",
                    });
                };
                if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}' | '\\') {
                    return Err(LexError {
                        span: Span {
                            start: pos,
                            end: pos + c.len_utf8(),
                        },
                        message: "string escapes and line terminators are not supported",
                    });
                }
                pos += c.len_utf8();
                if c == quote {
                    break;
                }
            }
            TokenKind::String
        } else {
            // Longest spelling wins. Slash is reserved until lexical goals exist.
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
        tokens.push(Token {
            kind,
            text: &source[start..pos],
            span: Span { start, end: pos },
            has_line_terminator_before,
        });
        has_line_terminator_before = false;
    }
    tokens.push(Token {
        kind: TokenKind::End,
        has_line_terminator_before,
        text: "",
        span: Span {
            start: pos,
            end: pos,
        },
    });
    Ok(tokens)
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
            "0xff",
            "123abc",
            "'a\\n'",
            "'a\nb'",
            "`template`",
            "名前",
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
