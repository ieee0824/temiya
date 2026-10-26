use temiya::ast::{Expr, ExprKind, Literal, ObjectProperty, TemplateElement};
use temiya::lexer::Span;
use temiya::source::SourceText;

fn name(start: usize, end: usize, name: &str) -> Expr {
    Expr::new(Span { start, end }, ExprKind::Identifier(name.to_owned()))
}

#[test]
fn ast_is_owned_and_can_outlive_the_source() {
    let expr = {
        let source = String::from("雪");
        let expr = name(0, source.len(), &source);
        expr.validate(&SourceText::new(&source)).unwrap();
        expr
    };
    assert_eq!(expr.kind, ExprKind::Identifier("雪".to_owned()));
    let clone = expr.clone();
    assert_eq!(expr, clone);
}

#[test]
fn visits_nested_operands_in_order_and_preserves_ranges() {
    let expr = Expr::new(
        Span { start: 0, end: 5 },
        ExprKind::Binary {
            operator: "+".to_owned(),
            left: Box::new(name(0, 1, "a")),
            right: Box::new(Expr::new(
                Span { start: 2, end: 5 },
                ExprKind::Parenthesized(Box::new(name(3, 4, "b"))),
            )),
        },
    );
    expr.validate(&SourceText::new("a+(b)")).unwrap();
    let mut spans = vec![];
    expr.visit(&mut |node| spans.push(node.span));
    assert_eq!(
        spans,
        [
            Span { start: 0, end: 5 },
            Span { start: 0, end: 1 },
            Span { start: 2, end: 5 },
            Span { start: 3, end: 4 }
        ]
    );
}

#[test]
fn invalid_and_noncontained_byte_ranges_are_rejected() {
    let source = SourceText::new("雪a");
    for span in [
        Span { start: 1, end: 3 },
        Span { start: 3, end: 1 },
        Span { start: 0, end: 9 },
    ] {
        assert_eq!(
            name(span.start, span.end, "x")
                .validate(&source)
                .unwrap_err()
                .span,
            span
        );
    }
    let expr = Expr::new(
        Span { start: 0, end: 3 },
        ExprKind::Parenthesized(Box::new(name(3, 4, "a"))),
    );
    assert_eq!(
        expr.validate(&source).unwrap_err().message,
        "child is outside its parent"
    );
    name(4, 4, "").validate(&source).unwrap(); // structural validation permits empty EOF spans
}

#[test]
fn validates_object_property_boundaries() {
    let expr = Expr::new(
        Span { start: 0, end: 5 },
        ExprKind::Object(vec![ObjectProperty {
            span: Span { start: 1, end: 4 },
            key: name(1, 2, "a"),
            value: Some(name(3, 4, "b")),
            computed: false,
            spread: false,
            shorthand: false,
            comma_after: false,
        }]),
    );
    expr.validate(&SourceText::new("{a:b}")).unwrap();
    let mut invalid = expr;
    if let ExprKind::Object(properties) = &mut invalid.kind {
        properties[0].span.end = 3;
    }
    assert_eq!(
        invalid
            .validate(&SourceText::new("{a:b}"))
            .unwrap_err()
            .message,
        "property child is outside its parent"
    );
}

#[test]
fn validates_template_count_and_preserves_lone_surrogates() {
    let mut expr = Expr::new(
        Span { start: 0, end: 8 },
        ExprKind::Template {
            tag: None,
            quasis: vec![TemplateElement {
                span: Span { start: 0, end: 8 },
                raw: vec![0xd800],
                cooked: None,
            }],
            expressions: vec![],
        },
    );
    expr.validate(&SourceText::new("`\\uD800`")).unwrap();
    if let ExprKind::Template { expressions, .. } = &mut expr.kind {
        expressions.push(name(1, 2, "x"));
    }
    assert_eq!(
        expr.validate(&SourceText::new("`\\uD800`"))
            .unwrap_err()
            .message,
        "template component count mismatch"
    );
    let literal = Expr::new(
        Span { start: 0, end: 8 },
        ExprKind::Literal(Literal::String {
            value: vec![0xd800],
            legacy_escape: false,
        }),
    );
    literal.validate(&SourceText::new("'\\uD800'")).unwrap();
}
