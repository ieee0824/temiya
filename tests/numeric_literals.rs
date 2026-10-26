use temiya::lexer::{Lexer, LexicalGoal, Span, TokenKind, tokenize};

#[test]
fn decimal_radices_separators_and_bigints() {
    for (kind, literals) in [
        (
            TokenKind::Number,
            vec![
                "0",
                "1",
                "1.",
                ".5",
                "1.25",
                "1e2",
                "1.E-2",
                ".1e+2",
                "1_000",
                "1_2.3_4e5_6",
                "0b10",
                "0B1_0",
                "0o77",
                "0O7_0",
                "0xff",
                "0Xf_F",
                "1e999999",
            ],
        ),
        (
            TokenKind::BigInt,
            vec![
                "0n",
                "123n",
                "1_000n",
                "0b1_0n",
                "0o7n",
                "0Xf_Fn",
                "123456789012345678901234567890n",
            ],
        ),
        (
            TokenKind::LegacyOctalNumber,
            vec!["00", "01", "077", "000777"],
        ),
        (
            TokenKind::LegacyDecimalNumber,
            vec![
                "08", "09", "0789", "08.5", "09e2", "08.e+2", "08.1_2", "08e1_2",
            ],
        ),
    ] {
        for literal in literals {
            let source = format!("\n{literal};");
            let tokens = tokenize(&source).unwrap();
            assert_eq!(tokens[0].kind, kind, "{literal}");
            assert_eq!(tokens[0].text, literal);
            assert_eq!(
                tokens[0].span,
                Span {
                    start: 1,
                    end: literal.len() + 1
                }
            );
            assert!(tokens[0].has_line_terminator_before);
            assert_eq!(tokens[1].text, ";");
        }
    }
}

#[test]
fn invalid_literals_and_adjacent_identifiers() {
    for literal in [
        "0b", "0o", "0x", "0B2", "0b102", "0o8", "0xg", "0x_1", "0b_1", "0o_1", "1_", "1__2",
        "0_1", "00_1", "0_0n", "0x1_", "1._2", "1_.2", "1e_2", "1_e2", "1e+_2", "1e", "1e+", "1e-",
        "1.2n", ".5n", "1e2n", "01n", "08n", "1nn", "0x1nn", "0n0", "1a", "1$", "1_foo", "1名前",
        "0x1π", r"1\u0061", "1nfoo", "0b1nfoo", "01e2",
    ] {
        let error = tokenize(literal).unwrap_err();
        assert!(error.span.start <= error.span.end, "{literal}");
        assert!(error.span.end <= literal.len(), "{literal}");
        assert!(literal.is_char_boundary(error.span.start));
        assert!(literal.is_char_boundary(error.span.end));
    }
}

#[test]
fn punctuation_whitespace_and_comments_end_literals() {
    for separator in [
        " ",
        "\t",
        "\n",
        "\r",
        "\u{2028}",
        "\u{2029}",
        "\u{00a0}",
        "/**/",
        "//comment\n",
    ] {
        let source = format!("123{separator}名前");
        let tokens = tokenize(&source).unwrap();
        assert_eq!(tokens[0].text, "123");
        assert_eq!(tokens[1].text, "名前");
    }
    assert_eq!(tokenize("-1n").unwrap()[0].text, "-");
    let tokens = tokenize("1..x").unwrap();
    assert_eq!(tokens[0].text, "1.");
    assert_eq!(tokens[1].text, ".");
    let tokens = tokenize("077.toString").unwrap();
    assert_eq!(tokens[0].kind, TokenKind::LegacyOctalNumber);
    assert_eq!(tokens[1].text, ".");
    let mut lexer = Lexer::new("0xff / 2");
    assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "0xff");
    assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "/");
}
