use temiya::lexer::{Lexer, LexicalGoal, Span, TokenKind, tokenize};

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn complete_templates_preserve_raw_and_cooked_values() {
    for (literal, raw, cooked) in [
        ("``", "", ""),
        (r"`a\nb`", r"a\nb", "a\nb"),
        (r"`\`\${x}`", r"\`\${x}", "`${x}"),
        ("`雪😀`", "雪😀", "雪😀"),
        ("`a\r\nb\rc\n`", "a\nb\nc\n", "a\nb\nc\n"),
        ("`a\\\r\nb`", "a\\\nb", "ab"),
        ("`\u{2028}\u{2029}`", "\u{2028}\u{2029}", "\u{2028}\u{2029}"),
    ] {
        let source = format!("\n{literal} ;");
        let tokens = tokenize(&source).unwrap();
        let token = tokens[0];
        assert_eq!(token.kind, TokenKind::Template);
        assert_eq!(token.text, literal);
        assert_eq!(
            token.span,
            Span {
                start: 1,
                end: literal.len() + 1
            }
        );
        let value = token.template_value().unwrap();
        assert_eq!(value.raw, utf16(raw));
        assert_eq!(value.cooked, Some(utf16(cooked)));
        assert!(!tokens[1].has_line_terminator_before);
    }
}

#[test]
fn caller_controls_nested_expression_and_template_boundaries() {
    let mut lexer = Lexer::new("`a${{x:`b${y}c`}}d${z}e`");
    for (goal, kind, raw, cooked) in [
        (LexicalGoal::Div, TokenKind::TemplateHead, "`a${", Some("a")),
        (LexicalGoal::Div, TokenKind::Punctuator, "{", None),
        (LexicalGoal::Div, TokenKind::Identifier, "x", None),
        (LexicalGoal::Div, TokenKind::Punctuator, ":", None),
        (LexicalGoal::Div, TokenKind::TemplateHead, "`b${", Some("b")),
        (LexicalGoal::Div, TokenKind::Identifier, "y", None),
        (
            LexicalGoal::TemplateTail,
            TokenKind::TemplateTail,
            "}c`",
            Some("c"),
        ),
        (LexicalGoal::Div, TokenKind::Punctuator, "}", None),
        (
            LexicalGoal::RegExpOrTemplateTail,
            TokenKind::TemplateMiddle,
            "}d${",
            Some("d"),
        ),
        (LexicalGoal::RegExp, TokenKind::Identifier, "z", None),
        (
            LexicalGoal::TemplateTail,
            TokenKind::TemplateTail,
            "}e`",
            Some("e"),
        ),
    ] {
        let token = lexer.next_token(goal).unwrap();
        assert_eq!(token.kind, kind);
        assert_eq!(token.text, raw);
        assert_eq!(
            token.template_value().and_then(|v| v.cooked),
            cooked.map(utf16)
        );
    }
    assert_eq!(
        lexer.next_token(LexicalGoal::Div).unwrap().kind,
        TokenKind::End
    );
}

#[test]
fn template_tail_goals_preserve_slash_choices_and_comments() {
    for (goal, text) in [
        (LexicalGoal::TemplateTail, "/"),
        (LexicalGoal::RegExpOrTemplateTail, "/x/"),
    ] {
        let mut lexer = Lexer::new("//comment\n/x/");
        let token = lexer.next_token(goal).unwrap();
        assert_eq!(token.text, text);
        assert!(token.has_line_terminator_before);
    }
    let token = Lexer::new("/*comment*/\n}tail`")
        .next_token(LexicalGoal::TemplateTail)
        .unwrap();
    assert!(token.has_line_terminator_before);
    assert_eq!(token.template_value().unwrap().cooked, Some(utf16("tail")));
}

#[test]
fn invalid_escapes_preserve_raw_values_for_tagged_templates() {
    for body in [
        r"\u{}",
        r"\u{110000}",
        r"\xG",
        r"\u123",
        r"\1",
        r"\00",
        r"\8",
        r"\9",
        r"\08",
    ] {
        let source = format!("`{body}`");
        let token = tokenize(&source).unwrap()[0];
        let value = token.template_value().unwrap();
        assert_eq!(value.raw, utf16(body));
        assert_eq!(value.cooked, None);
    }
    let token = tokenize(r"`\uD800\0`").unwrap()[0];
    assert_eq!(
        token.template_value().unwrap().cooked,
        Some(vec![0xd800, 0])
    );
    let token = tokenize(r"`\x${").unwrap()[0];
    assert_eq!(token.kind, TokenKind::TemplateHead);
    assert_eq!(token.template_value().unwrap().cooked, None);
}

#[test]
fn unclosed_components_have_byte_spans_and_do_not_advance() {
    for (source, goal) in [
        ("`", LexicalGoal::Div),
        ("`雪", LexicalGoal::Div),
        ("`\\", LexicalGoal::Div),
        ("}tail", LexicalGoal::TemplateTail),
    ] {
        let mut lexer = Lexer::new(source);
        let error = lexer.next_token(goal).unwrap_err();
        assert_eq!(
            error.span,
            Span {
                start: 0,
                end: source.len()
            }
        );
        assert_eq!(lexer.next_token(goal).unwrap_err(), error);
    }
}
