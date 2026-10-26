use temiya::lexer::{Lexer, LexicalGoal, Span, TokenKind};

#[test]
fn same_source_can_be_division_or_regexp() {
    let source = "/ab+c/gi";
    let regexp = Lexer::new(source).next_token(LexicalGoal::RegExp).unwrap();
    assert_eq!(regexp.kind, TokenKind::RegularExpression);
    assert_eq!(regexp.text, source);
    assert_eq!(
        regexp.span,
        Span {
            start: 0,
            end: source.len()
        }
    );

    let mut lexer = Lexer::new(source);
    for expected in ["/", "ab", "+", "c", "/", "gi", ""] {
        let token = lexer.next_token(LexicalGoal::Div).unwrap();
        assert_eq!(token.text, expected);
        assert_ne!(token.kind, TokenKind::RegularExpression);
    }
}

#[test]
fn caller_switches_goals_for_each_token() {
    let mut lexer = Lexer::new("value /= /ab+c/gi / 2");
    for (goal, kind, text) in [
        (LexicalGoal::Div, TokenKind::Identifier, "value"),
        (LexicalGoal::Div, TokenKind::Punctuator, "/="),
        (
            LexicalGoal::RegExp,
            TokenKind::RegularExpression,
            "/ab+c/gi",
        ),
        (LexicalGoal::Div, TokenKind::Punctuator, "/"),
        (LexicalGoal::Div, TokenKind::Number, "2"),
        (LexicalGoal::RegExp, TokenKind::End, ""),
    ] {
        let token = lexer.next_token(goal).unwrap();
        assert_eq!((token.kind, token.text), (kind, text));
    }
}

#[test]
fn preserves_regexp_spelling_and_utf8_spans() {
    for literal in [
        "/ab+c/gi", "/[/]/", r"/a\/b/", r"/[\]]/", r"/\\/", "/[]/", "/[^]/", "/(?:)/", "/=/",
        "/雪/", "/a*/", "/[[]/",
    ] {
        let source = format!("/*手宮*/\n{literal};");
        let mut lexer = Lexer::new(&source);
        let token = lexer.next_token(LexicalGoal::RegExp).unwrap();
        assert_eq!(token.kind, TokenKind::RegularExpression);
        assert_eq!(token.text, literal);
        assert_eq!(
            token.span,
            Span {
                start: 11,
                end: 11 + literal.len()
            }
        );
        assert_eq!(&source[token.span.start..token.span.end], literal);
        assert!(token.has_line_terminator_before);
        let next = lexer.next_token(LexicalGoal::Div).unwrap();
        assert_eq!(next.text, ";");
        assert!(!next.has_line_terminator_before);
    }
}

#[test]
fn flags_follow_identifier_part_characters_without_semantic_validation() {
    for flags in [
        "",
        "g",
        "gg",
        "invalid",
        "g1_$",
        "雪",
        "\u{0301}",
        "\u{200c}\u{200d}",
        "\u{2118}",
    ] {
        let literal = format!("/x/{flags}");
        let source = format!("{literal}+next");
        let mut lexer = Lexer::new(&source);
        let token = lexer.next_token(LexicalGoal::RegExp).unwrap();
        assert_eq!(token.text, literal);
        assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "+");
    }
    // Identifier escapes are separate tokens rather than part of flags.
    // A future parser rejects their adjacency to a regexp literal.
    let mut lexer = Lexer::new(r"/x/\u0067");
    assert_eq!(lexer.next_token(LexicalGoal::RegExp).unwrap().text, "/x/");
    assert_eq!(
        lexer
            .next_token(LexicalGoal::Div)
            .unwrap()
            .identifier_name()
            .as_deref(),
        Some("g")
    );
    let mut lexer = Lexer::new("/x/😀");
    assert_eq!(lexer.next_token(LexicalGoal::RegExp).unwrap().text, "/x/");
    assert!(lexer.next_token(LexicalGoal::Div).is_err());
}

#[test]
fn comments_take_precedence_in_both_goals() {
    for goal in [LexicalGoal::Div, LexicalGoal::RegExp] {
        for source in [
            "//",
            "//empty body is a comment",
            "/**/",
            "/*雪*/ //trailing",
        ] {
            assert_eq!(
                Lexer::new(source).next_token(goal).unwrap().kind,
                TokenKind::End
            );
        }
        let token = Lexer::new("//comment\n/*comment*/ /x/")
            .next_token(goal)
            .unwrap();
        assert!(token.has_line_terminator_before);
        assert_eq!(
            token.text,
            if goal == LexicalGoal::Div { "/" } else { "/x/" }
        );
        assert!(Lexer::new("/*unterminated").next_token(goal).is_err());
    }
}

#[test]
fn unterminated_bodies_classes_and_escapes_have_spans() {
    for source in ["/", "/abc", "/[", "/[abc/", "/[[]", "/abc\\", "/[abc\\"] {
        let error = Lexer::new(source)
            .next_token(LexicalGoal::RegExp)
            .unwrap_err();
        assert_eq!(
            error.span,
            Span {
                start: 0,
                end: source.len()
            },
            "{source:?}"
        );
        assert!(error.message.contains("unterminated"));
    }
}

#[test]
fn line_terminators_are_rejected_even_when_escaped_or_in_classes() {
    for newline in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
        for prefix in ["/a", "/[a", "/a\\", "/[a\\"] {
            let source = format!("{prefix}{newline}/");
            let error = Lexer::new(&source)
                .next_token(LexicalGoal::RegExp)
                .unwrap_err();
            assert_eq!(
                error.span,
                Span {
                    start: prefix.len(),
                    end: prefix.len() + newline.chars().next().unwrap().len_utf8(),
                }
            );
            assert_eq!(
                error.message,
                "line terminator in regular-expression literal"
            );
        }
    }
}

#[test]
fn errors_do_not_advance_and_eof_consumes_trailing_trivia_once() {
    let mut lexer = Lexer::new("\n/=");
    let error = lexer.next_token(LexicalGoal::RegExp).unwrap_err();
    assert_eq!(lexer.next_token(LexicalGoal::RegExp).unwrap_err(), error);
    let token = lexer.next_token(LexicalGoal::Div).unwrap();
    assert_eq!(token.text, "/=");
    assert!(token.has_line_terminator_before);

    let source = "/x/ /*\n*/";
    let mut lexer = Lexer::new(source);
    lexer.next_token(LexicalGoal::RegExp).unwrap();
    let end = lexer.next_token(LexicalGoal::Div).unwrap();
    assert_eq!(end.kind, TokenKind::End);
    assert!(end.has_line_terminator_before);
    assert_eq!(
        end.span,
        Span {
            start: source.len(),
            end: source.len()
        }
    );
    let repeated = lexer.next_token(LexicalGoal::RegExp).unwrap();
    assert_eq!(repeated.span, end.span);
    assert!(!repeated.has_line_terminator_before);
    assert!(
        !Lexer::new("")
            .next_token(LexicalGoal::Div)
            .unwrap()
            .has_line_terminator_before
    );
}
