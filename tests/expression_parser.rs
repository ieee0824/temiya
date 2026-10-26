use temiya::ast::{Expr, ExprKind, Literal};
use temiya::lexer::{LexerOptions, SourceGoal};
use temiya::parser::{ParserOptions, parse_expression, parse_expression_with_options};
use temiya::source::SourceText;

fn snapshot(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Identifier(name) => name.clone(),
        ExprKind::Literal(Literal::Number { raw, .. }) => raw.clone(),
        ExprKind::Parenthesized(e) => format!("(paren {})", snapshot(e)),
        ExprKind::Unary { operator, argument } => format!("({operator} {})", snapshot(argument)),
        ExprKind::Update {
            operator,
            argument,
            prefix,
        } => format!(
            "({}{} {})",
            if *prefix { "pre" } else { "post" },
            operator,
            snapshot(argument)
        ),
        ExprKind::Binary {
            operator,
            left,
            right,
        } => format!("({operator} {} {})", snapshot(left), snapshot(right)),
        ExprKind::Assignment {
            operator,
            target,
            value,
        } => format!("({operator} {} {})", snapshot(target), snapshot(value)),
        ExprKind::Conditional {
            test,
            consequent,
            alternate,
        } => format!(
            "(?: {} {} {})",
            snapshot(test),
            snapshot(consequent),
            snapshot(alternate)
        ),
        ExprKind::Sequence(items) => format!(
            "(seq {})",
            items.iter().map(snapshot).collect::<Vec<_>>().join(" ")
        ),
        _ => format!("{:?}", expr.kind),
    }
}

#[test]
fn pratt_precedence_and_associativity_snapshots() {
    for (source, expected) in [
        ("a+b*c", "(+ a (* b c))"),
        ("a-b-c", "(- (- a b) c)"),
        ("a**b**c", "(** a (** b c))"),
        ("a=b=c", "(= a (= b c))"),
        ("a+=b*=c", "(+= a (*= b c))"),
        ("a?b:c?d:e", "(?: a b (?: c d e))"),
        ("a?b=c:d=e", "(?: a (= b c) (= d e))"),
        ("a,b,c=d", "(seq a b (= c d))"),
        (
            "a||b&&c|d^e&f==g<h<<i+j*k",
            "(|| a (&& b (| c (^ d (& e (== f (< g (<< h (+ i (* j k))))))))))",
        ),
        ("a??b??c", "(?? (?? a b) c)"),
        ("!a + ~b", "(+ (! a) (~ b))"),
        ("typeof a + void b", "(+ (typeof a) (void b))"),
        ("++a + b--", "(+ (pre++ a) (post-- b))"),
        ("a ** -b", "(** a (- b))"),
        ("(-a) ** b", "(** (paren (- a)) b)"),
    ] {
        let expr = parse_expression(source).unwrap();
        assert_eq!(snapshot(&expr), expected, "{source}");
        expr.validate(&SourceText::new(source)).unwrap();
    }
}

#[test]
fn all_operator_families_parse_with_owned_spelling() {
    for operator in [
        "+",
        "-",
        "*",
        "/",
        "%",
        "**",
        "<<",
        ">>",
        ">>>",
        "<",
        ">",
        "<=",
        ">=",
        "instanceof",
        "in",
        "==",
        "!=",
        "===",
        "!==",
        "&",
        "|",
        "^",
        "&&",
        "||",
        "??",
        "=",
        "+=",
        "-=",
        "*=",
        "/=",
        "%=",
        "**=",
        "<<=",
        ">>=",
        ">>>=",
        "&=",
        "|=",
        "^=",
        "&&=",
        "||=",
        "??=",
    ] {
        let source = format!("a {operator} b");
        assert_eq!(
            snapshot(&parse_expression(&source).unwrap()),
            format!("({operator} a b)")
        );
    }
}

#[test]
fn primary_member_call_array_object_template_and_new_forms() {
    for source in [
        "null",
        "true",
        "false",
        "this",
        "'雪\\uD800'",
        "0xffn",
        "/a[\\/]b/gi",
        r"\u0061",
        "a.b[c](d,...e,)",
        "a?.b?.[c]?.(d)",
        "a?.3:0",
        "new C(1).x",
        "new ns.C(1)(2)",
        "new new C()",
        "new tag`x`",
        "[,,1,...items,]",
        "({a, class:1, ['x'+y]:2, ...rest})",
        "({cl\\u0061ss:1})",
        "`a${{x:`b${y}c`}.x}d`",
        r"tag`\xG${a}b`",
        "(a??b)||c",
        "a??(b&&c)",
        "(a?.b).c=1",
        "a / /x/.source.length",
    ] {
        let expr = parse_expression(source).unwrap_or_else(|error| panic!("{source}: {error}"));
        expr.validate(&SourceText::new(source))
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        assert_eq!(expr.span.start, 0);
        assert_eq!(expr.span.end, source.len());
    }
}

#[test]
fn grammar_errors_have_spans() {
    for source in [
        "",
        "a b",
        "a+",
        "a?b",
        "()",
        "(a",
        "a[",
        "a.",
        "a?.",
        "a(,b)",
        "[1 2]",
        "({1})",
        "({[x]})",
        "1=x",
        "a+b=1",
        "a?.b=1",
        "a?.b.c=1",
        "a?.b++",
        "++1",
        "++a++",
        "-a**b",
        "a||b??c",
        "a??b&&c",
        "a&&b??c",
        "new a?.b()",
        "a?.b`x`",
        r"`\xG`",
        r"cl\u0061ss",
        "a\n++",
    ] {
        let error = parse_expression(source).unwrap_err();
        assert!(error.span.start <= error.span.end, "{source}");
        assert!(error.span.end <= source.len(), "{source}");
    }
}

#[test]
fn strict_module_and_recursion_options_are_applied() {
    let strict = ParserOptions {
        strict: true,
        ..ParserOptions::default()
    };
    for source in ["077", "08", r"'\1'", "let", "yield", "delete x"] {
        assert!(parse_expression(source).is_ok(), "{source}");
        assert!(
            parse_expression_with_options(source, strict).is_err(),
            "{source}"
        );
    }
    let module = ParserOptions {
        lexer: LexerOptions {
            source_goal: SourceGoal::Module,
            html_comments: true,
        },
        ..ParserOptions::default()
    };
    assert!(parse_expression_with_options("await", module).is_err());
    assert!(parse_expression_with_options("08", module).is_err());
    assert!(
        parse_expression_with_options(
            "a",
            ParserOptions {
                max_depth: 0,
                ..ParserOptions::default()
            }
        )
        .is_err()
    );
    let nested = format!("{}a{}", "(".repeat(200), ")".repeat(200));
    assert_eq!(
        parse_expression(&nested).unwrap_err().message,
        "expression nesting limit exceeded"
    );
}
