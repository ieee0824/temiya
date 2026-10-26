use temiya::{
    lexer::tokenize,
    parser::{parse_expression, parse_module, parse_script},
    source::SourceText,
};

const REGRESSIONS: &[&str] = &[include_str!(
    "../fuzz/regressions/lexer_parser/escaped-line.js"
)];

#[test]
fn lexer_parser_regression_corpus_never_panics_or_corrupts_ranges() {
    for source in REGRESSIONS {
        if let Ok(tokens) = tokenize(source) {
            for token in tokens {
                assert!(token.span.start <= token.span.end && token.span.end <= source.len());
                assert!(source.is_char_boundary(token.span.start));
                assert!(source.is_char_boundary(token.span.end));
            }
        }
        if let Ok(expression) = parse_expression(source) {
            expression.validate(&SourceText::new(source)).unwrap();
        }
        for program in [parse_script(source), parse_module(source)]
            .into_iter()
            .flatten()
        {
            program.validate(&SourceText::new(source)).unwrap();
        }
    }
}

#[test]
fn parser_errors_stay_on_utf8_boundaries() {
    for source in ["雪 +", "`𐐀${", "#私的", "\\u{10ffff}"] {
        for error in [parse_script(source).err(), parse_module(source).err()]
            .into_iter()
            .flatten()
        {
            assert!(error.span.start <= error.span.end && error.span.end <= source.len());
            assert!(source.is_char_boundary(error.span.start));
            assert!(source.is_char_boundary(error.span.end));
        }
    }
}
