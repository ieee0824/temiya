use temiya::lexer::Span;
use temiya::source::{PositionError, SourcePosition, SourceText};

#[test]
fn utf16_positions_include_non_bmp_characters() {
    let source = SourceText::new("a雪😀b");
    for (byte, offset) in [(0, 0), (1, 1), (4, 2), (8, 4), (9, 5)] {
        assert_eq!(
            source.position(byte).unwrap(),
            SourcePosition {
                offset,
                line: 0,
                column: offset
            }
        );
        assert_eq!(source.byte_offset(offset).unwrap(), byte);
        assert_eq!(source.byte_offset_at(0, offset).unwrap(), byte);
    }
    let range = source.range(Span { start: 1, end: 8 }).unwrap();
    assert_eq!(range.start.offset, 1);
    assert_eq!(range.end.offset, 4);
    assert_eq!(source.text(), "a雪😀b");
}

#[test]
fn all_line_separators_have_consistent_positions() {
    for newline in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
        let text = format!("😀{newline}雪{newline}");
        let source = SourceText::new(&text);
        let second_line = source.position(4 + newline.len()).unwrap();
        assert_eq!(second_line.line, 1);
        assert_eq!(second_line.column, 0);
        assert_eq!(second_line.offset, 2 + newline.encode_utf16().count());
        let end = source.position(text.len()).unwrap();
        assert_eq!((end.line, end.column), (2, 0));
        for byte in 0..=text.len() {
            if text.is_char_boundary(byte) {
                let position = source.position(byte).unwrap();
                assert_eq!(source.byte_offset(position.offset).unwrap(), byte);
                assert_eq!(
                    source
                        .byte_offset_at(position.line, position.column)
                        .unwrap(),
                    byte
                );
            }
        }
    }
}

#[test]
fn crlf_interior_is_on_the_preceding_line() {
    let source = SourceText::new("a\r\nb");
    assert_eq!(
        source.position(2).unwrap(),
        SourcePosition {
            offset: 2,
            line: 0,
            column: 2
        }
    );
    assert_eq!(
        source.position(3).unwrap(),
        SourcePosition {
            offset: 3,
            line: 1,
            column: 0
        }
    );
}

#[test]
fn empty_source_and_zero_length_ranges_are_valid() {
    let source = SourceText::new("");
    let position = SourcePosition {
        offset: 0,
        line: 0,
        column: 0,
    };
    assert_eq!(source.position(0), Ok(position));
    assert_eq!(source.byte_offset(0), Ok(0));
    assert_eq!(source.byte_offset_at(0, 0), Ok(0));
    let range = source.range(Span { start: 0, end: 0 }).unwrap();
    assert_eq!(range.start, range.end);
}

#[test]
fn invalid_positions_return_explicit_errors() {
    let source = SourceText::new("雪😀\n");
    for byte in [1, 2, 4, 5, 6] {
        assert_eq!(source.position(byte), Err(PositionError::InsideCharacter));
    }
    assert_eq!(source.byte_offset(2), Err(PositionError::InsideCharacter));
    for offset in [9, usize::MAX] {
        assert_eq!(source.position(offset), Err(PositionError::OutOfBounds));
        assert_eq!(source.byte_offset(offset), Err(PositionError::OutOfBounds));
    }
    for (line, column) in [(0, 2), (1, 1), (2, 0), (usize::MAX, usize::MAX)] {
        assert_eq!(
            source.byte_offset_at(line, column),
            Err(PositionError::InvalidLineColumn)
        );
    }
    assert_eq!(
        source.range(Span { start: 3, end: 0 }),
        Err(PositionError::ReversedRange)
    );
    assert_eq!(
        source.range(Span { start: 0, end: 1 }),
        Err(PositionError::InsideCharacter)
    );
    assert_eq!(
        source.range(Span { start: 0, end: 9 }),
        Err(PositionError::OutOfBounds)
    );
}
