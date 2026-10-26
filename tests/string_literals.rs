use temiya::lexer::{Span, TokenKind, tokenize};

#[test]
fn escapes_decode_to_utf16_without_changing_raw_text() {
    for (raw, expected) in [
        (r"''", vec![]),
        (r#""a\n\r\t\b\f\v\0""#, vec![97, 10, 13, 9, 8, 12, 11, 0]),
        (r#"'\'\"\\\/'"#, vec![39, 34, 92, 47]),
        (r"'\x41\u0042\u{43}'", vec![65, 66, 67]),
        (r"'\u{1F600}'", vec![0xd83d, 0xde00]),
        ("'雪😀'", vec![0x96ea, 0xd83d, 0xde00]),
        (r"'\uD800\u{DFFF}'", vec![0xd800, 0xdfff]),
        (r"'\q\雪'", vec![113, 0x96ea]),
        ("'\u{2028}\u{2029}'", vec![0x2028, 0x2029]),
    ] {
        let source = format!("\n{raw};");
        let tokens = tokenize(&source).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::String, "{raw}");
        assert_eq!(tokens[0].text, raw);
        assert_eq!(
            tokens[0].span,
            Span {
                start: 1,
                end: raw.len() + 1
            }
        );
        assert_eq!(tokens[0].string_value(), Some(expected));
        assert!(!tokens[1].has_line_terminator_before);
    }
    assert_eq!(tokenize("1").unwrap()[0].string_value(), None);
}

#[test]
fn line_continuations_are_removed_from_cooked_value() {
    for newline in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
        let raw = format!("'a\\{newline}b'");
        let source = format!("{raw} c");
        let tokens = tokenize(&source).unwrap();
        assert_eq!(tokens[0].text, raw);
        assert_eq!(tokens[0].string_value(), Some(vec![97, 98]));
        assert!(!tokens[1].has_line_terminator_before);
    }
}

#[test]
fn legacy_escapes_remain_distinguishable_for_strict_parsing() {
    for (raw, expected) in [
        (r"'\1'", vec![1]),
        (r"'\01'", vec![1]),
        (r"'\000'", vec![0]),
        (r"'\377'", vec![255]),
        (r"'\400'", vec![32, 48]),
        (r"'\777'", vec![63, 55]),
        (r"'\08'", vec![0, 56]),
        (r"'\8\9'", vec![56, 57]),
    ] {
        let token = tokenize(raw).unwrap()[0];
        assert_eq!(token.kind, TokenKind::LegacyString, "{raw}");
        assert_eq!(token.string_value(), Some(expected));
    }
    assert_eq!(tokenize(r"'\0x'").unwrap()[0].kind, TokenKind::String);
}

#[test]
fn invalid_escapes_and_unclosed_literals_report_source_ranges() {
    for raw in [
        "'",
        "'abc",
        "'\\",
        "'a\nb'",
        "'a\rb'",
        r"'\x'",
        r"'\x0'",
        r"'\xGG'",
        r"'\u123'",
        r"'\u{}'",
        r"'\u{110000}'",
        r"'\u{abc'",
    ] {
        let error = tokenize(raw).unwrap_err();
        assert!(error.span.start <= error.span.end);
        assert!(error.span.end <= raw.len());
        assert!(raw.is_char_boundary(error.span.start));
        assert!(raw.is_char_boundary(error.span.end));
    }
    assert_eq!(
        tokenize("'a\nb'").unwrap_err().span,
        Span { start: 2, end: 3 }
    );
    assert_eq!(
        tokenize("'abc").unwrap_err().span,
        Span { start: 0, end: 4 }
    );
}
