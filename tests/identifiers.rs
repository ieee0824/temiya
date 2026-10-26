use std::borrow::Cow;
use temiya::lexer::{Lexer, LexicalGoal, Span, TokenKind, tokenize};

#[test]
fn unicode_identifier_properties_and_raw_spans() {
    for name in [
        "名前",
        "π2",
        "𐐀x",
        "a\u{0301}",
        "a\u{200c}\u{200d}",
        "_",
        "$",
        "℘",
        "a·b",
    ] {
        let source = format!("\n{name}+1");
        let mut lexer = Lexer::new(&source);
        let token = lexer.next_token(LexicalGoal::Div).unwrap();
        assert_eq!(token.kind, TokenKind::Identifier, "{name}");
        assert_eq!(token.text, name);
        assert_eq!(token.identifier_name(), Some(Cow::Borrowed(name)));
        assert_eq!(
            token.span,
            Span {
                start: 1,
                end: 1 + name.len()
            }
        );
        assert!(token.has_line_terminator_before);
        assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "+");
    }
}

#[test]
fn identifier_escapes_decode_without_normalization() {
    for (raw, expected) in [
        (r"\u0061bc", "abc"),
        (r"a\u0062c", "abc"),
        (r"\u{540d}\u524d", "名前"),
        (r"\u{10400}x", "𐐀x"),
        (r"a\u200c\u{200d}", "a\u{200c}\u{200d}"),
        (r"\u005f\u0024", "_$"),
        (r"a\u{0000000000000301}", "a\u{0301}"),
    ] {
        let tokens = tokenize(raw).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Identifier);
        assert_eq!(tokens[0].text, raw);
        assert_eq!(tokens[0].span.end, raw.len());
        assert_eq!(tokens[0].identifier_name().as_deref(), Some(expected));
        assert!(matches!(tokens[0].identifier_name(), Some(Cow::Owned(_))));
    }
    let a = tokenize("é").unwrap();
    let b = tokenize("e\u{0301}").unwrap();
    assert_ne!(a[0].identifier_name(), b[0].identifier_name());
}

#[test]
fn escaped_reserved_words_are_not_keyword_tokens() {
    for (raw, name) in [
        (r"cl\u0061ss", "class"),
        (r"\u0069f", "if"),
        (r"\u{0065}num", "enum"),
        (r"tr\u0075e", "true"),
    ] {
        let token = tokenize(raw).unwrap()[0];
        assert_eq!(token.kind, TokenKind::EscapedKeyword);
        assert_eq!(token.identifier_name().as_deref(), Some(name));
        assert_eq!(tokenize(name).unwrap()[0].kind, TokenKind::Keyword);
    }
    for raw in ["let", "await", "yield", "async", r"\u006cet", r"aw\u0061it"] {
        assert_eq!(tokenize(raw).unwrap()[0].kind, TokenKind::Identifier);
    }
    assert_eq!(tokenize("classy").unwrap()[0].kind, TokenKind::Identifier);
}

#[test]
fn invalid_escapes_and_identifier_positions_are_rejected() {
    for raw in [
        r"\",
        r"\x61",
        r"\u",
        r"\u123",
        r"\uZZZZ",
        r"\u{}",
        r"\u{61",
        r"\u{110000}",
        r"\uD800",
        r"\u{DFFF}",
        r"\u0030abc",
        r"a\u0020",
        r"\u200c",
        r"\u200d",
        r"\u0301",
        "\u{200c}a",
        "\u{0301}a",
        "😀",
    ] {
        let error = tokenize(raw).unwrap_err();
        assert!(error.span.start <= error.span.end);
        assert!(error.span.end <= raw.len());
        assert!(raw.is_char_boundary(error.span.start));
        assert!(raw.is_char_boundary(error.span.end));
    }
    let error = tokenize(r"a\u0020").unwrap_err();
    assert_eq!(error.span, Span { start: 1, end: 7 });
    assert_eq!(tokenize("1").unwrap()[0].identifier_name(), None);
    assert_eq!(tokenize("").unwrap()[0].identifier_name(), None);
}
