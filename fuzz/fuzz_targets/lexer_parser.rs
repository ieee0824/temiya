#![no_main]

use libfuzzer_sys::fuzz_target;
use temiya::{
    lexer::tokenize,
    parser::{parse_expression, parse_module, parse_script},
    source::SourceText,
};

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    match tokenize(source) {
        Ok(tokens) => {
            for token in tokens {
                assert!(token.span.start <= token.span.end);
                assert!(token.span.end <= source.len());
                assert!(source.is_char_boundary(token.span.start));
                assert!(source.is_char_boundary(token.span.end));
                assert_eq!(token.text, &source[token.span.start..token.span.end]);
            }
        }
        Err(error) => assert_valid_span(source, error.span.start, error.span.end),
    }
    match parse_expression(source) {
        Ok(expression) => expression.validate(&SourceText::new(source)).unwrap(),
        Err(error) => assert_valid_span(source, error.span.start, error.span.end),
    }
    for parsed in [parse_script(source), parse_module(source)] {
        match parsed {
            Ok(program) => program.validate(&SourceText::new(source)).unwrap(),
            Err(error) => assert_valid_span(source, error.span.start, error.span.end),
        }
    }
});

fn assert_valid_span(source: &str, start: usize, end: usize) {
    assert!(start <= end);
    assert!(end <= source.len());
    assert!(source.is_char_boundary(start));
    assert!(source.is_char_boundary(end));
}
