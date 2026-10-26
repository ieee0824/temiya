//! Expression and statement parsing with explicit lexical goals and owned syntax.

use crate::ast::{Expr, ExprKind, Literal, NumberKind, ObjectProperty, TemplateElement};
use crate::lexer::{
    LexError, Lexer, LexerOptions, LexicalGoal, SourceGoal, Span, Token, TokenKind,
};

mod statements;
pub use statements::{parse_async_function_body, parse_function_body, parse_program, parse_script};

/// Parsing configuration for source context and recursion limits.
#[derive(Clone, Copy, Debug)]
pub struct ParserOptions {
    /// Source goal and optional HTML comment policy.
    pub lexer: LexerOptions,
    /// Enables strict-code restrictions on legacy literals and identifiers.
    /// Module source is always strict regardless of this setting.
    pub strict: bool,
    /// Maximum nested statement/expression parsing calls; zero rejects all nodes.
    pub max_depth: usize,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            lexer: LexerOptions::default(),
            strict: false,
            max_depth: 64,
        }
    }
}

/// A syntax or lexical error with its original UTF-8 source range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    /// Offending token or source range. EOF errors have an empty span.
    pub span: Span,
    /// Description of the failure.
    pub message: &'static str,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}
impl std::error::Error for ParseError {}
impl From<LexError> for ParseError {
    fn from(error: LexError) -> Self {
        Self {
            span: error.span,
            message: error.message,
        }
    }
}

/// Parses one complete expression with default Script options.
///
/// Rejects trailing tokens. The returned AST owns all names and literal data.
pub fn parse_expression(source: &str) -> Result<Expr, ParseError> {
    parse_expression_with_options(source, ParserOptions::default())
}

/// Parses one complete expression with explicit source context and limits.
pub fn parse_expression_with_options(
    source: &str,
    options: ParserOptions,
) -> Result<Expr, ParseError> {
    let mut parser = Parser::new(source, options);
    let expr = parser.expression(0)?;
    let end = parser.take(LexicalGoal::Div)?;
    if end.kind != TokenKind::End {
        return Err(error(end.span, "unexpected token after expression"));
    }
    Ok(expr)
}

fn error(span: Span, message: &'static str) -> ParseError {
    ParseError { span, message }
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    options: ParserOptions,
    depth: usize,
    last_end: usize,
    allow_in: bool,
    loops: usize,
    switches: usize,
    labels: Vec<(String, bool)>,
    in_function: bool,
    in_async: bool,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, options: ParserOptions) -> Self {
        Self {
            lexer: Lexer::with_options(source, options.lexer),
            options,
            depth: 0,
            last_end: 0,
            allow_in: true,
            loops: 0,
            switches: 0,
            labels: vec![],
            in_function: false,
            in_async: options.lexer.source_goal == SourceGoal::Module,
        }
    }

    fn expression_with_in(&mut self, min_bp: u8, allow_in: bool) -> Result<Expr, ParseError> {
        let previous = self.allow_in;
        self.allow_in = allow_in;
        let result = self.expression(min_bp);
        self.allow_in = previous;
        result
    }
    fn strict(&self) -> bool {
        self.options.strict || self.options.lexer.source_goal == SourceGoal::Module
    }

    fn peek(&self, goal: LexicalGoal) -> Result<Token<'a>, ParseError> {
        Ok(self.lexer.clone().next_token(goal)?)
    }

    fn take(&mut self, goal: LexicalGoal) -> Result<Token<'a>, ParseError> {
        let token = self.lexer.next_token(goal)?;
        self.last_end = token.span.end;
        Ok(token)
    }

    fn expect(&mut self, text: &str) -> Result<Token<'a>, ParseError> {
        let token = self.take(LexicalGoal::Div)?;
        if token.text != text {
            return Err(error(token.span, "unexpected token"));
        }
        Ok(token)
    }

    fn at(&self, text: &str) -> Result<bool, ParseError> {
        Ok(self.peek(LexicalGoal::Div)?.text == text)
    }

    fn node(&self, start: usize, kind: ExprKind) -> Expr {
        Expr::new(
            Span {
                start,
                end: self.last_end,
            },
            kind,
        )
    }

    fn expression(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        if self.depth >= self.options.max_depth {
            return Err(error(
                self.peek(LexicalGoal::RegExp)?.span,
                "expression nesting limit exceeded",
            ));
        }
        self.depth += 1;
        let result = self.expression_inner(min_bp);
        self.depth -= 1;
        result
    }

    fn expression_inner(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut left = self.primary()?;
        loop {
            let token = self.peek(LexicalGoal::Div)?;
            let start = left.span.start;
            match token.text {
                "." | "[" | "?." if min_bp <= 20 => {
                    let optional = token.text == "?.";
                    self.take(LexicalGoal::Div)?;
                    if optional && self.at("(")? {
                        let arguments = self.arguments()?;
                        left = self.node(
                            start,
                            ExprKind::Call {
                                callee: Box::new(left),
                                arguments,
                                optional: true,
                            },
                        );
                        continue;
                    }
                    let computed = token.text == "[" || (optional && self.at("[")?);
                    let property = if computed {
                        if optional {
                            self.expect("[")?;
                        }
                        let property = self.expression_with_in(0, true)?;
                        self.expect("]")?;
                        property
                    } else {
                        self.property_name()?
                    };
                    left = self.node(
                        start,
                        ExprKind::Member {
                            object: Box::new(left),
                            property: Box::new(property),
                            computed,
                            optional,
                        },
                    );
                    continue;
                }
                "(" if min_bp <= 19 => {
                    let arguments = self.arguments()?;
                    left = self.node(
                        start,
                        ExprKind::Call {
                            callee: Box::new(left),
                            arguments,
                            optional: false,
                        },
                    );
                    continue;
                }
                "++" | "--" if min_bp <= 17 && !token.has_line_terminator_before => {
                    if !assignable(&left) {
                        return Err(error(left.span, "invalid update target"));
                    }
                    self.take(LexicalGoal::Div)?;
                    left = self.node(
                        start,
                        ExprKind::Update {
                            operator: token.text.to_owned(),
                            argument: Box::new(left),
                            prefix: false,
                        },
                    );
                    continue;
                }
                _ => {}
            }
            if matches!(token.kind, TokenKind::Template | TokenKind::TemplateHead) && min_bp <= 20 {
                if optional_chain(&left) {
                    return Err(error(left.span, "optional chain cannot tag a template"));
                }
                left = self.template(Some(left))?;
                continue;
            }
            if token.text == "?" && min_bp <= 3 {
                self.take(LexicalGoal::Div)?;
                let consequent = self.expression_with_in(2, true)?;
                self.expect(":")?;
                let alternate = self.expression(2)?;
                left = self.node(
                    start,
                    ExprKind::Conditional {
                        test: Box::new(left),
                        consequent: Box::new(consequent),
                        alternate: Box::new(alternate),
                    },
                );
                continue;
            }
            let Some((lbp, rbp)) = binding_power(token.text) else {
                break;
            };
            if token.text == "in" && !self.allow_in {
                break;
            }
            if lbp < min_bp {
                break;
            }
            self.take(LexicalGoal::Div)?;
            if lbp == 2 && !assignable(&left) {
                return Err(error(left.span, "invalid assignment target"));
            }
            if token.text == "**" && matches!(left.kind, ExprKind::Unary { .. }) {
                return Err(error(
                    left.span,
                    "unparenthesized unary expression before exponentiation",
                ));
            }
            let right = self.expression(rbp)?;
            if mixed_nullish(token.text, &left) || mixed_nullish(token.text, &right) {
                return Err(error(
                    token.span,
                    "nullish and logical operators require parentheses",
                ));
            }
            left = if lbp == 1 {
                let mut items = match left.kind {
                    ExprKind::Sequence(items) => items,
                    _ => vec![left],
                };
                items.push(right);
                self.node(start, ExprKind::Sequence(items))
            } else if lbp == 2 {
                self.node(
                    start,
                    ExprKind::Assignment {
                        operator: token.text.to_owned(),
                        target: Box::new(left),
                        value: Box::new(right),
                    },
                )
            } else {
                self.node(
                    start,
                    ExprKind::Binary {
                        operator: token.text.to_owned(),
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                )
            };
        }
        Ok(left)
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek(LexicalGoal::RegExp)?;
        let start = token.span.start;
        if matches!(token.kind, TokenKind::Template | TokenKind::TemplateHead) {
            return self.template(None);
        }
        self.take(LexicalGoal::RegExp)?;
        let kind = match token.text {
            "(" => {
                let expr = self.expression_with_in(0, true)?;
                self.expect(")")?;
                ExprKind::Parenthesized(Box::new(expr))
            }
            "[" => return self.array(start),
            "{" => return self.object(start),
            "!" | "~" | "+" | "-" | "typeof" | "void" | "delete" => {
                let argument = self.expression(17)?;
                if token.text == "delete" && self.strict() && identifier_target(&argument) {
                    return Err(error(
                        argument.span,
                        "cannot delete an identifier in strict code",
                    ));
                }
                ExprKind::Unary {
                    operator: token.text.to_owned(),
                    argument: Box::new(argument),
                }
            }
            "++" | "--" => {
                let argument = self.expression(17)?;
                if !assignable(&argument) {
                    return Err(error(argument.span, "invalid update target"));
                }
                ExprKind::Update {
                    operator: token.text.to_owned(),
                    argument: Box::new(argument),
                    prefix: true,
                }
            }
            "await" if self.in_async => {
                let argument = self.expression(17)?;
                ExprKind::Unary {
                    operator: "await".to_owned(),
                    argument: Box::new(argument),
                }
            }
            "new" => {
                let callee = self.expression(20)?;
                if optional_chain(&callee) {
                    return Err(error(callee.span, "optional chain cannot be a constructor"));
                }
                let arguments = if self.at("(")? {
                    self.arguments()?
                } else {
                    vec![]
                };
                ExprKind::New {
                    callee: Box::new(callee),
                    arguments,
                }
            }
            "this" => ExprKind::This,
            "null" => ExprKind::Literal(Literal::Null),
            "true" | "false" => ExprKind::Literal(Literal::Boolean(token.text == "true")),
            _ => return self.literal_or_identifier(token),
        };
        Ok(self.node(start, kind))
    }

    fn literal_or_identifier(&self, token: Token<'a>) -> Result<Expr, ParseError> {
        let kind = match token.kind {
            TokenKind::Identifier => {
                let name = token.identifier_name().unwrap().into_owned();
                if (self.strict()
                    && matches!(
                        name.as_str(),
                        "implements"
                            | "interface"
                            | "package"
                            | "private"
                            | "protected"
                            | "public"
                            | "static"
                            | "let"
                            | "yield"
                    ))
                    || (self.in_async && name == "await")
                {
                    return Err(error(token.span, "reserved identifier in this context"));
                }
                ExprKind::Identifier(name)
            }
            TokenKind::String | TokenKind::LegacyString => {
                let legacy_escape = token.kind == TokenKind::LegacyString;
                if legacy_escape && self.strict() {
                    return Err(error(token.span, "legacy string escape in strict code"));
                }
                ExprKind::Literal(Literal::String {
                    value: token.string_value().unwrap(),
                    legacy_escape,
                })
            }
            TokenKind::Number
            | TokenKind::BigInt
            | TokenKind::LegacyOctalNumber
            | TokenKind::LegacyDecimalNumber => {
                let kind = match token.kind {
                    TokenKind::BigInt => NumberKind::BigInt,
                    TokenKind::LegacyOctalNumber => NumberKind::LegacyOctal,
                    TokenKind::LegacyDecimalNumber => NumberKind::LegacyDecimal,
                    _ => NumberKind::Number,
                };
                if self.strict()
                    && matches!(kind, NumberKind::LegacyOctal | NumberKind::LegacyDecimal)
                {
                    return Err(error(token.span, "legacy number in strict code"));
                }
                ExprKind::Literal(Literal::Number {
                    raw: token.text.to_owned(),
                    kind,
                })
            }
            TokenKind::RegularExpression => {
                ExprKind::Literal(Literal::RegExp(token.text.to_owned()))
            }
            _ => return Err(error(token.span, "expected expression")),
        };
        Ok(Expr::new(token.span, kind))
    }

    fn property_name(&mut self) -> Result<Expr, ParseError> {
        let token = self.take(LexicalGoal::Div)?;
        if let Some(name) = token.identifier_name() {
            return Ok(Expr::new(
                token.span,
                ExprKind::Identifier(name.into_owned()),
            ));
        }
        Err(error(token.span, "expected property name"))
    }

    fn arguments(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect("(")?;
        let mut arguments = vec![];
        while !self.at(")")? {
            let argument = self.element()?;
            arguments.push(argument);
            if !self.at(",")? {
                break;
            }
            self.expect(",")?;
        }
        self.expect(")")?;
        Ok(arguments)
    }

    fn element(&mut self) -> Result<Expr, ParseError> {
        if self.at("...")? {
            let start = self.expect("...")?.span.start;
            let expr = self.expression_with_in(2, true)?;
            Ok(self.node(start, ExprKind::Spread(Box::new(expr))))
        } else {
            self.expression_with_in(2, true)
        }
    }

    fn array(&mut self, start: usize) -> Result<Expr, ParseError> {
        let mut items = vec![];
        while !self.at("]")? {
            if self.at(",")? {
                self.expect(",")?;
                items.push(None);
                continue;
            }
            items.push(Some(self.element()?));
            if !self.at(",")? {
                break;
            }
            self.expect(",")?;
        }
        self.expect("]")?;
        Ok(self.node(start, ExprKind::Array(items)))
    }

    fn object(&mut self, start: usize) -> Result<Expr, ParseError> {
        let mut properties = vec![];
        while !self.at("}")? {
            let token = self.peek(LexicalGoal::Div)?;
            let spread = token.text == "...";
            let computed = token.text == "[";
            let key = if spread {
                self.expect("...")?;
                self.expression_with_in(2, true)?
            } else if computed {
                self.expect("[")?;
                let key = self.expression_with_in(2, true)?;
                self.expect("]")?;
                key
            } else if token.identifier_name().is_some() {
                self.property_name()?
            } else {
                let token = self.take(LexicalGoal::Div)?;
                self.literal_or_identifier(token)?
            };
            let value = if !spread && self.at(":")? {
                self.expect(":")?;
                Some(self.expression_with_in(2, true)?)
            } else {
                if !spread && (computed || token.kind != TokenKind::Identifier) {
                    return Err(error(key.span, "expected property value"));
                }
                if !spread {
                    self.literal_or_identifier(token)?;
                }
                None
            };
            properties.push(ObjectProperty {
                span: Span {
                    start: token.span.start,
                    end: self.last_end,
                },
                key,
                value,
                computed,
                spread,
            });
            if !self.at(",")? {
                break;
            }
            self.expect(",")?;
        }
        self.expect("}")?;
        Ok(self.node(start, ExprKind::Object(properties)))
    }

    fn template(&mut self, tag: Option<Expr>) -> Result<Expr, ParseError> {
        let token = self.take(LexicalGoal::RegExp)?;
        let start = tag.as_ref().map_or(token.span.start, |tag| tag.span.start);
        let mut token = token;
        let mut quasis = vec![];
        let mut expressions = vec![];
        loop {
            let value = token.template_value().unwrap();
            if tag.is_none() && value.cooked.is_none() {
                return Err(error(token.span, "invalid escape in untagged template"));
            }
            quasis.push(TemplateElement {
                span: token.span,
                raw: value.raw,
                cooked: value.cooked,
            });
            if matches!(token.kind, TokenKind::Template | TokenKind::TemplateTail) {
                break;
            }
            expressions.push(self.expression_with_in(0, true)?);
            token = self.take(LexicalGoal::TemplateTail)?;
            if !matches!(
                token.kind,
                TokenKind::TemplateMiddle | TokenKind::TemplateTail
            ) {
                return Err(error(token.span, "expected template continuation"));
            }
        }
        Ok(self.node(
            start,
            ExprKind::Template {
                tag: tag.map(Box::new),
                quasis,
                expressions,
            },
        ))
    }
}

fn binding_power(operator: &str) -> Option<(u8, u8)> {
    let (power, right) = match operator {
        "," => (1, false),
        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "**=" | "<<=" | ">>=" | ">>>=" | "&=" | "|="
        | "^=" | "&&=" | "||=" | "??=" => (2, true),
        "??" => (4, false),
        "||" => (5, false),
        "&&" => (6, false),
        "|" => (7, false),
        "^" => (8, false),
        "&" => (9, false),
        "==" | "!=" | "===" | "!==" => (10, false),
        "<" | ">" | "<=" | ">=" | "in" | "instanceof" => (11, false),
        "<<" | ">>" | ">>>" => (12, false),
        "+" | "-" => (13, false),
        "*" | "/" | "%" => (14, false),
        "**" => (16, true),
        _ => return None,
    };
    Some((power, if right { power } else { power + 1 }))
}

fn mixed_nullish(operator: &str, expr: &Expr) -> bool {
    let ExprKind::Binary {
        operator: child, ..
    } = &expr.kind
    else {
        return false;
    };
    (operator == "??" && matches!(child.as_str(), "&&" | "||"))
        || (matches!(operator, "&&" | "||") && child == "??")
}

fn optional_chain(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Member {
            optional, object, ..
        } => *optional || optional_chain(object),
        ExprKind::Call {
            optional, callee, ..
        } => *optional || optional_chain(callee),
        _ => false,
    }
}

fn identifier_target(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Identifier(_) => true,
        ExprKind::Parenthesized(expr) => identifier_target(expr),
        _ => false,
    }
}

fn assignable(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Identifier(_) => true,
        ExprKind::Member { .. } => !optional_chain(expr),
        ExprKind::Parenthesized(expr) => assignable(expr),
        _ => false,
    }
}
