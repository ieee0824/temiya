use temiya::lexer::{Lexer, LexerOptions, LexicalGoal, SourceGoal, TokenKind};

fn lexer(source: &str, source_goal: SourceGoal, html_comments: bool) -> Lexer<'_> {
    Lexer::with_options(
        source,
        LexerOptions {
            source_goal,
            html_comments,
        },
    )
}

#[test]
fn hashbang_is_valid_only_at_absolute_start_in_both_source_goals() {
    for source_goal in [SourceGoal::Script, SourceGoal::Module] {
        for newline in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
            let source = format!("#!/usr/bin/env temiya{newline}answer");
            let token = lexer(&source, source_goal, false)
                .next_token(LexicalGoal::Div)
                .unwrap();
            assert_eq!(token.text, "answer");
            assert_eq!(token.span.start, source.len() - 6);
            assert!(token.has_line_terminator_before);
        }
        assert_eq!(
            lexer("#!", source_goal, false)
                .next_token(LexicalGoal::Div)
                .unwrap()
                .kind,
            TokenKind::End
        );
        for prefix in [" ", "\n", "\u{feff}", "/**/"] {
            let source = format!("{prefix}#!command");
            assert!(
                lexer(&source, source_goal, false)
                    .next_token(LexicalGoal::Div)
                    .is_err()
            );
        }
        let mut lexer = lexer("x\n#!command", source_goal, false);
        lexer.next_token(LexicalGoal::Div).unwrap();
        assert!(lexer.next_token(LexicalGoal::Div).is_err());
    }
}

#[test]
fn html_open_comments_require_script_and_opt_in() {
    for (source_goal, enabled) in [
        (SourceGoal::Module, false),
        (SourceGoal::Module, true),
        (SourceGoal::Script, false),
    ] {
        let first = lexer("<!--comment\nanswer", source_goal, enabled)
            .next_token(LexicalGoal::Div)
            .unwrap();
        assert_eq!(first.text, "<");
    }
    let mut lexer = lexer("x<!--comment\nanswer", SourceGoal::Script, true);
    assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "x");
    let next = lexer.next_token(LexicalGoal::RegExp).unwrap();
    assert_eq!(next.text, "answer");
    assert!(next.has_line_terminator_before);
}

#[test]
fn html_close_comments_require_a_line_prefix_of_trivia() {
    for prefix in ["", " \t", "/**/ ", "/*a\nb*/", "\u{3000}"] {
        let source = format!("{prefix}-->comment\nanswer");
        let token = lexer(&source, SourceGoal::Script, true)
            .next_token(LexicalGoal::Div)
            .unwrap();
        assert_eq!(token.text, "answer");
        assert!(token.has_line_terminator_before);
    }
    for newline in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}", "/*\n*/"] {
        let source = format!("x{newline} /**/ -->comment\nanswer");
        let mut lexer = lexer(&source, SourceGoal::Script, true);
        lexer.next_token(LexicalGoal::Div).unwrap();
        assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "answer");
    }
    for (source_goal, enabled, source) in [
        (SourceGoal::Script, true, "x-->y"),
        (SourceGoal::Script, true, "x/**/-->y"),
        (SourceGoal::Module, true, "x\n-->y"),
        (SourceGoal::Script, false, "x\n-->y"),
    ] {
        let mut lexer = lexer(source, source_goal, enabled);
        assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "x");
        assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "--");
        assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, ">");
        assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "y");
    }
}

#[test]
fn html_delimiters_inside_literals_are_preserved() {
    let mut lexer = lexer("'<!--' `-->` /<!--/", SourceGoal::Script, true);
    assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "'<!--'");
    assert_eq!(lexer.next_token(LexicalGoal::Div).unwrap().text, "`-->`");
    assert_eq!(
        lexer.next_token(LexicalGoal::RegExp).unwrap().text,
        "/<!--/"
    );
}
