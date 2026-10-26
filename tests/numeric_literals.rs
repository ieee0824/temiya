use temiya::lexer::{Lexer, LexicalGoal, Span, TokenKind, tokenize};

#[test]
fn engine_evaluates_spelling_variants_and_numeric_boundaries() {
    use temiya::{Engine, runtime::Value};
    let engine = Engine::default();
    let realm = engine.create_realm();
    for (source, expected) in [
        ("42", 42.0),
        ("4_2", 42.0),
        ("0b101010", 42.0),
        ("0B10_1010", 42.0),
        ("0o52", 42.0),
        ("0O5_2", 42.0),
        ("0x2a", 42.0),
        ("0X2_A", 42.0),
        (".42e2", 42.0),
        ("4_2.0_0", 42.0),
        ("052", 42.0),
        ("09", 9.0),
        ("1e999", f64::INFINITY),
        ("5e-324", f64::from_bits(1)),
        ("-0", -0.0),
        ("9007199254740993", 9007199254740992.0),
    ] {
        let script = engine.compile(&realm, source).unwrap();
        let result = engine.evaluate(&script).unwrap();
        assert!(
            result.value().same_value(&Value::Number(expected)),
            "{source}: {result:?}"
        );
    }
    for source in [
        "42n",
        "4_2n",
        "0b101010n",
        "0B10_1010n",
        "0o52n",
        "0O5_2n",
        "0x2an",
        "0X2_An",
    ] {
        let result = engine
            .evaluate(&engine.compile(&realm, source).unwrap())
            .unwrap();
        assert!(
            result.value().same_value(&Value::BigInt(42.into())),
            "{source}: {result:?}"
        );
    }
    let large = Value::BigInt((num_bigint::BigInt::from(1u8) << 256usize) - 1u8);
    for source in [
        "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffn",
        "0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFFn",
    ] {
        let result = engine
            .evaluate(&engine.compile(&realm, source).unwrap())
            .unwrap();
        assert!(result.value().same_value(&large), "{source}: {result:?}");
    }
}

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
