use super::*;
use crate::ast::{Stmt, StmtKind};
use std::collections::HashSet;

impl<'a> Parser<'a> {
    pub(super) fn nested_function(&self, asynchronous: bool, generator: bool, arrow: bool) -> Self {
        let mut nested = self.clone();
        nested.in_function = true;
        nested.in_async = asynchronous;
        nested.in_generator = generator;
        nested.in_parameters = false;
        nested.await_reserved =
            asynchronous || self.options.lexer.source_goal == SourceGoal::Module;
        nested.allow_in = true;
        nested.loops = 0;
        nested.switches = 0;
        nested.labels.clear();
        if !arrow {
            nested.super_property = false;
            nested.super_call = false;
            nested.new_target = true;
            nested.forbid_arguments = false;
        }
        nested
    }

    pub(super) fn commit_cursor(&mut self, nested: Self) {
        self.lexer = nested.lexer;
        self.last_end = nested.last_end;
    }

    pub(super) fn is_async_function(&self) -> Result<bool, ParseError> {
        let mut lexer = self.lexer.clone();
        if lexer.next_token(LexicalGoal::RegExp)?.text != "async" {
            return Ok(false);
        }
        let next = lexer.next_token(LexicalGoal::Div)?;
        Ok(next.text == "function" && !next.has_line_terminator_before)
    }

    pub(super) fn function(
        &mut self,
        declaration: bool,
        asynchronous: bool,
        start: usize,
    ) -> Result<Function, ParseError> {
        self.expect("function")?;
        let generator = self.at("*")?;
        if generator {
            self.expect("*")?;
        }
        let mut nested = self.nested_function(asynchronous, generator, false);
        let name = if nested.peek(LexicalGoal::Div)?.kind == TokenKind::Identifier {
            if declaration {
                let name = self.binding_identifier()?;
                nested.lexer = self.lexer.clone();
                nested.last_end = self.last_end;
                Some(name)
            } else {
                Some(nested.binding_identifier()?)
            }
        } else if declaration {
            return Err(error(
                nested.peek(LexicalGoal::Div)?.span,
                "function declaration requires a name",
            ));
        } else {
            None
        };
        let parameters = nested.parameters()?;
        let (block, strict_directive) = nested.function_block()?;
        let strict = nested.strict();
        nested.check_parameters(&parameters, strict, false, strict_directive)?;
        if strict {
            if let Some(name) = &name {
                check_strict_binding(name)?;
            }
        }
        let function = Function {
            span: Span {
                start,
                end: nested.last_end,
            },
            name,
            parameters,
            body: FunctionBody::Block(Box::new(block)),
            asynchronous,
            generator,
            strict,
        };
        self.commit_cursor(nested);
        Ok(function)
    }

    pub(super) fn function_block(&mut self) -> Result<(Stmt, bool), ParseError> {
        let start = self.expect("{")?.span.start;
        let mut body = vec![];
        let mut prologue = true;
        let mut legacy = None;
        let mut strict_directive = false;
        while !self.at("}")? {
            let stmt = self.statement_list_item()?;
            if prologue {
                if let StmtKind::Expression(Expr {
                    span,
                    kind: ExprKind::Literal(Literal::String { legacy_escape, .. }),
                }) = &stmt.kind
                {
                    if *legacy_escape {
                        legacy = Some(*span);
                    }
                    if matches!(
                        &self.source[span.start..span.end],
                        "'use strict'" | "\"use strict\""
                    ) {
                        strict_directive = true;
                        self.options.strict = true;
                        if let Some(span) = legacy {
                            return Err(error(span, "legacy escape in strict directive prologue"));
                        }
                    }
                } else {
                    prologue = false;
                }
            }
            body.push(stmt);
        }
        self.expect("}")?;
        Ok((
            Stmt {
                span: Span {
                    start,
                    end: self.last_end,
                },
                kind: StmtKind::Block(body),
            },
            strict_directive,
        ))
    }

    pub(super) fn parameters(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect("(")?;
        self.in_parameters = true;
        let mut parameters = vec![];
        while !self.at(")")? {
            let rest = self.at("...")?;
            let parameter = if rest {
                let start = self.expect("...")?.span.start;
                let binding = self.binding()?;
                self.node(start, ExprKind::Spread(Box::new(binding)))
            } else {
                self.binding_element()?
            };
            parameters.push(parameter);
            if rest && !self.at(")")? {
                return Err(error(
                    self.peek(LexicalGoal::Div)?.span,
                    "rest parameter must be last without a default or trailing comma",
                ));
            }
            if !self.at(",")? {
                break;
            }
            self.expect(",")?;
        }
        self.expect(")")?;
        self.in_parameters = false;
        Ok(parameters)
    }

    pub(super) fn check_parameters(
        &self,
        parameters: &[Expr],
        strict: bool,
        unique: bool,
        strict_directive: bool,
    ) -> Result<(), ParseError> {
        let simple = parameters
            .iter()
            .all(|expr| matches!(expr.kind, ExprKind::Identifier(_)));
        if !simple && strict_directive {
            return Err(error(
                parameters[0].span,
                "strict directive with non-simple parameters",
            ));
        }
        let mut seen = HashSet::new();
        for parameter in parameters {
            for (name, span) in bound_names(parameter) {
                if (unique || strict || !simple) && !seen.insert(name) {
                    return Err(error(span, "duplicate parameter name"));
                }
                if strict && strict_binding_name(name) {
                    return Err(error(span, "invalid strict parameter"));
                }
                if (self.in_async && name == "await") || (self.in_generator && name == "yield") {
                    return Err(error(span, "reserved parameter name"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn binding(&mut self) -> Result<Expr, ParseError> {
        if self.depth >= self.options.max_depth {
            return Err(error(
                self.peek(LexicalGoal::Div)?.span,
                "binding nesting limit exceeded",
            ));
        }
        self.depth += 1;
        let result = self.binding_inner();
        self.depth -= 1;
        result
    }

    fn binding_inner(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek(LexicalGoal::Div)?;
        let start = token.span.start;
        if token.text == "[" {
            self.expect("[")?;
            let mut elements = vec![];
            let mut trailing_comma = false;
            while !self.at("]")? {
                if self.at(",")? {
                    self.expect(",")?;
                    elements.push(None);
                    trailing_comma = false;
                    continue;
                }
                let rest = self.at("...")?;
                let element = if rest {
                    let start = self.expect("...")?.span.start;
                    let binding = self.binding()?;
                    self.node(start, ExprKind::Spread(Box::new(binding)))
                } else {
                    self.binding_element()?
                };
                elements.push(Some(element));
                trailing_comma = false;
                if rest && !self.at("]")? {
                    return Err(error(
                        self.peek(LexicalGoal::Div)?.span,
                        "array rest must be last without a trailing comma",
                    ));
                }
                if !self.at(",")? {
                    break;
                }
                self.expect(",")?;
                trailing_comma = true;
            }
            self.expect("]")?;
            return Ok(self.node(
                start,
                ExprKind::Array {
                    elements,
                    trailing_comma,
                },
            ));
        }
        if token.text == "{" {
            self.expect("{")?;
            let mut properties = vec![];
            while !self.at("}")? {
                let token = self.peek(LexicalGoal::Div)?;
                let spread = token.text == "...";
                let mut shorthand = false;
                let (key, computed, value) = if spread {
                    self.expect("...")?;
                    (self.binding_identifier()?, false, None)
                } else {
                    let (key, computed) = self.definition_key(false)?;
                    let value = if self.at(":")? {
                        self.expect(":")?;
                        Some(self.binding_element()?)
                    } else {
                        if computed || token.kind != TokenKind::Identifier {
                            return Err(error(key.span, "invalid shorthand binding"));
                        }
                        if self.strict() {
                            check_strict_binding(&key)?;
                        }
                        self.literal_or_identifier(token)?;
                        shorthand = true;
                        if self.at("=")? {
                            self.expect("=")?;
                            let value = self.expression_with_in(2, true)?;
                            Some(self.node(
                                key.span.start,
                                ExprKind::Assignment {
                                    operator: "=".to_owned(),
                                    target: Box::new(key.clone()),
                                    value: Box::new(value),
                                },
                            ))
                        } else {
                            None
                        }
                    };
                    (key, computed, value)
                };
                properties.push(ObjectProperty {
                    span: Span {
                        start: token.span.start,
                        end: self.last_end,
                    },
                    key,
                    computed,
                    value,
                    spread,
                    shorthand,
                    comma_after: false,
                });
                if spread && !self.at("}")? {
                    return Err(error(
                        self.peek(LexicalGoal::Div)?.span,
                        "object rest must be last without a trailing comma",
                    ));
                }
                if !self.at(",")? {
                    break;
                }
                self.expect(",")?;
                properties.last_mut().unwrap().comma_after = true;
            }
            self.expect("}")?;
            return Ok(self.node(start, ExprKind::Object(properties)));
        }
        self.binding_identifier()
    }

    fn binding_element(&mut self) -> Result<Expr, ParseError> {
        let binding = self.binding()?;
        if !self.at("=")? {
            return Ok(binding);
        }
        self.expect("=")?;
        let value = self.expression_with_in(2, true)?;
        Ok(self.node(
            binding.span.start,
            ExprKind::Assignment {
                operator: "=".to_owned(),
                target: Box::new(binding),
                value: Box::new(value),
            },
        ))
    }

    pub(super) fn try_arrow(&mut self) -> Result<Option<Expr>, ParseError> {
        let first = self.peek(LexicalGoal::RegExp)?;
        let mut probe = self.clone();
        let mut asynchronous = false;
        if first.text == "async" {
            probe.take(LexicalGoal::Div)?;
            let next = probe.peek(LexicalGoal::Div)?;
            if !next.has_line_terminator_before && next.text != "=>" {
                asynchronous = true;
            } else {
                probe = self.clone();
            }
        }
        let mut nested = probe.nested_function(asynchronous, false, true);
        nested.in_async |= self.in_async;
        nested.in_generator = self.in_generator;
        nested.await_reserved |= self.await_reserved;
        let parsed = if nested.at("(")? {
            nested.parameters()
        } else if nested.peek(LexicalGoal::Div)?.kind == TokenKind::Identifier {
            nested.binding_identifier().map(|parameter| vec![parameter])
        } else {
            return Ok(None);
        };
        let Ok(parameters) = parsed else {
            return Ok(None);
        };
        let next = nested.peek(LexicalGoal::Div)?;
        if next.text != "=>" {
            return Ok(None);
        }
        if next.has_line_terminator_before {
            return Err(error(next.span, "line terminator before arrow"));
        }
        nested.expect("=>")?;
        nested.check_parameters(&parameters, nested.strict(), true, false)?;
        nested.in_async = asynchronous;
        nested.in_generator = false;
        nested.await_reserved =
            asynchronous || self.options.lexer.source_goal == SourceGoal::Module;
        let (body, strict_directive) = if nested.at("{")? {
            let (block, directive) = nested.function_block()?;
            (FunctionBody::Block(Box::new(block)), directive)
        } else {
            (
                FunctionBody::Expression(Box::new(nested.expression(2)?)),
                false,
            )
        };
        nested.check_parameters(&parameters, nested.strict(), true, strict_directive)?;
        let function = Function {
            span: Span {
                start: first.span.start,
                end: nested.last_end,
            },
            name: None,
            parameters,
            body,
            asynchronous,
            generator: false,
            strict: nested.strict(),
        };
        self.commit_cursor(nested);
        Ok(Some(self.node(
            first.span.start,
            ExprKind::Arrow(Box::new(function)),
        )))
    }
}

pub(super) fn bound_names(expr: &Expr) -> Vec<(&str, Span)> {
    match &expr.kind {
        ExprKind::Identifier(name) => vec![(name, expr.span)],
        ExprKind::Assignment { target, .. } | ExprKind::Spread(target) => bound_names(target),
        ExprKind::Array { elements, .. } => {
            elements.iter().flatten().flat_map(bound_names).collect()
        }
        ExprKind::Object(properties) => properties
            .iter()
            .flat_map(|p| bound_names(p.value.as_ref().unwrap_or(&p.key)))
            .collect(),
        _ => vec![],
    }
}

fn strict_binding_name(name: &str) -> bool {
    matches!(
        name,
        "eval"
            | "arguments"
            | "implements"
            | "interface"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "let"
            | "yield"
    )
}

fn check_strict_binding(expr: &Expr) -> Result<(), ParseError> {
    for (name, span) in bound_names(expr) {
        if strict_binding_name(name) {
            return Err(error(span, "invalid strict binding"));
        }
    }
    Ok(())
}
