use super::*;
use crate::ast::{
    CatchClause, ForInit, Program, Stmt, StmtKind, SwitchCase, VariableDeclarator, VariableKind,
};

/// Parses complete Script source using default options.
pub fn parse_script(source: &str) -> Result<Program, ParseError> {
    parse_program(source, ParserOptions::default())
}

/// Parses a complete source unit using explicit source and strict-mode options.
/// Unsupported syntax produces an error rather than an incomplete tree.
pub fn parse_program(source: &str, options: ParserOptions) -> Result<Program, ParseError> {
    body(
        source,
        options,
        false,
        options.lexer.source_goal == SourceGoal::Module,
    )
}

/// Parses a normal function body's contents, without surrounding braces.
/// Return statements are allowed; this does not parse function parameters.
pub fn parse_function_body(source: &str, options: ParserOptions) -> Result<Program, ParseError> {
    body(source, options, true, false)
}

/// Parses an async function body's contents, permitting await and for-await-of.
pub fn parse_async_function_body(
    source: &str,
    options: ParserOptions,
) -> Result<Program, ParseError> {
    body(source, options, true, true)
}

fn body(
    source: &str,
    options: ParserOptions,
    in_function: bool,
    in_async: bool,
) -> Result<Program, ParseError> {
    let mut parser = Parser::new(source, options);
    parser.in_function = in_function;
    parser.in_async = in_async;
    parser.await_reserved = in_async || options.lexer.source_goal == SourceGoal::Module;
    parser.new_target = in_function;
    let mut statements = vec![];
    let mut directives = true;
    let mut legacy_directives = vec![];
    while parser.peek(LexicalGoal::RegExp)?.kind != TokenKind::End {
        let stmt = parser.statement_list_item()?;
        if directives {
            if let StmtKind::Expression(Expr {
                span,
                kind: ExprKind::Literal(Literal::String { legacy_escape, .. }),
            }) = &stmt.kind
            {
                if *legacy_escape {
                    legacy_directives.push(*span);
                }
                if matches!(
                    &source[span.start..span.end],
                    "\"use strict\"" | "'use strict'"
                ) {
                    parser.options.strict = true;
                    if let Some(span) = legacy_directives.first() {
                        return Err(error(*span, "legacy escape in strict directive prologue"));
                    }
                }
            } else {
                directives = false;
            }
        }
        statements.push(stmt);
    }
    Ok(Program {
        span: Span {
            start: 0,
            end: source.len(),
        },
        body: statements,
        strict: parser.strict(),
    })
}

impl Parser<'_> {
    fn statement(&mut self) -> Result<Stmt, ParseError> {
        let stmt = self.statement_list_item()?;
        if matches!(
            stmt.kind,
            StmtKind::Variable {
                kind: VariableKind::Let | VariableKind::Const,
                ..
            } | StmtKind::FunctionDeclaration(_)
                | StmtKind::ClassDeclaration(_)
        ) {
            return Err(error(
                stmt.span,
                "lexical declaration requires a statement list",
            ));
        }
        Ok(stmt)
    }

    pub(super) fn statement_list_item(&mut self) -> Result<Stmt, ParseError> {
        if self.depth >= self.options.max_depth {
            return Err(error(
                self.peek(LexicalGoal::RegExp)?.span,
                "statement nesting limit exceeded",
            ));
        }
        self.depth += 1;
        let result = self.statement_inner();
        self.depth -= 1;
        result
    }

    fn stmt(&self, start: usize, kind: StmtKind) -> Stmt {
        Stmt {
            span: Span {
                start,
                end: self.last_end,
            },
            kind,
        }
    }

    fn statement_inner(&mut self) -> Result<Stmt, ParseError> {
        let token = self.peek(LexicalGoal::RegExp)?;
        let start = token.span.start;
        let kind = match token.text {
            "function" => {
                StmtKind::FunctionDeclaration(Box::new(self.function(true, false, start)?))
            }
            "async" if self.is_async_function()? => {
                self.expect("async")?;
                StmtKind::FunctionDeclaration(Box::new(self.function(true, true, start)?))
            }
            "class" => StmtKind::ClassDeclaration(Box::new(self.class(true)?)),
            ";" => {
                self.expect(";")?;
                StmtKind::Empty
            }
            "{" => return self.block(),
            "if" => {
                self.expect("if")?;
                let test = self.condition()?;
                let consequent = Box::new(self.statement()?);
                let alternate = if self.at("else")? {
                    self.expect("else")?;
                    Some(Box::new(self.statement()?))
                } else {
                    None
                };
                StmtKind::If {
                    test,
                    consequent,
                    alternate,
                }
            }
            "while" => {
                self.expect("while")?;
                let test = self.condition()?;
                let body = self.loop_body()?;
                StmtKind::While { test, body }
            }
            "do" => {
                self.expect("do")?;
                let body = self.loop_body()?;
                self.expect("while")?;
                let test = self.condition()?;
                if self.at(";")? {
                    self.expect(";")?;
                }
                StmtKind::DoWhile { body, test }
            }
            "for" => return self.for_statement(),
            "switch" => return self.switch_statement(),
            "try" => return self.try_statement(),
            "break" | "continue" => {
                self.take(LexicalGoal::Div)?;
                let next = self.peek(LexicalGoal::Div)?;
                let label =
                    if !next.has_line_terminator_before && next.kind == TokenKind::Identifier {
                        self.take(LexicalGoal::Div)?;
                        self.literal_or_identifier(next)?;
                        Some(next.identifier_name().unwrap().into_owned())
                    } else {
                        None
                    };
                if let Some(name) = &label {
                    let Some((_, iteration)) =
                        self.labels.iter().rev().find(|(label, _)| label == name)
                    else {
                        return Err(error(next.span, "unknown label"));
                    };
                    if token.text == "continue" && !iteration {
                        return Err(error(next.span, "continue label does not target a loop"));
                    }
                } else if self.loops == 0 && (token.text == "continue" || self.switches == 0) {
                    return Err(error(
                        token.span,
                        "break or continue outside a permitted context",
                    ));
                }
                self.semicolon()?;
                if token.text == "break" {
                    StmtKind::Break(label)
                } else {
                    StmtKind::Continue(label)
                }
            }
            "return" => {
                self.expect("return")?;
                if !self.in_function {
                    return Err(error(token.span, "return outside a function"));
                }
                let next = self.peek(LexicalGoal::RegExp)?;
                let value = if next.has_line_terminator_before
                    || next.kind == TokenKind::End
                    || matches!(next.text, ";" | "}")
                {
                    None
                } else {
                    Some(self.expression(0)?)
                };
                self.semicolon()?;
                StmtKind::Return(value)
            }
            "throw" => {
                self.expect("throw")?;
                let next = self.peek(LexicalGoal::RegExp)?;
                if next.has_line_terminator_before {
                    return Err(error(next.span, "line terminator after throw"));
                }
                let value = self.expression(0)?;
                self.semicolon()?;
                StmtKind::Throw(value)
            }
            "debugger" => {
                self.expect("debugger")?;
                self.semicolon()?;
                StmtKind::Debugger
            }
            "with" => {
                self.expect("with")?;
                if self.strict() {
                    return Err(error(token.span, "with in strict code"));
                }
                let object = self.condition()?;
                let body = Box::new(self.statement()?);
                StmtKind::With { object, body }
            }
            "var" | "const" => {
                let (kind, declarations) = self.variables(false)?;
                self.semicolon()?;
                StmtKind::Variable { kind, declarations }
            }
            "let" if self.is_lexical_declaration()? => {
                let (kind, declarations) = self.variables(false)?;
                self.semicolon()?;
                StmtKind::Variable { kind, declarations }
            }
            _ => {
                if token.kind == TokenKind::Identifier && self.is_label()? {
                    return self.labeled();
                }
                let expr = self.expression(0)?;
                self.semicolon()?;
                StmtKind::Expression(expr)
            }
        };
        Ok(self.stmt(start, kind))
    }

    pub(super) fn semicolon(&mut self) -> Result<(), ParseError> {
        let token = self.peek(LexicalGoal::Div)?;
        if token.text == ";" {
            self.take(LexicalGoal::Div)?;
        } else if token.kind != TokenKind::End
            && token.text != "}"
            && !token.has_line_terminator_before
        {
            return Err(error(token.span, "expected semicolon"));
        }
        Ok(())
    }

    fn condition(&mut self) -> Result<Expr, ParseError> {
        self.expect("(")?;
        let expr = self.expression_with_in(0, true)?;
        self.expect(")")?;
        Ok(expr)
    }

    pub(super) fn block(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect("{")?.span.start;
        let mut statements = vec![];
        while !self.at("}")? {
            if self.peek(LexicalGoal::RegExp)?.kind == TokenKind::End {
                return Err(error(
                    self.peek(LexicalGoal::Div)?.span,
                    "unterminated block",
                ));
            }
            statements.push(self.statement_list_item()?);
        }
        self.expect("}")?;
        Ok(self.stmt(start, StmtKind::Block(statements)))
    }

    fn loop_body(&mut self) -> Result<Box<Stmt>, ParseError> {
        self.loops += 1;
        let result = self.statement();
        self.loops -= 1;
        result.map(Box::new)
    }

    fn is_label(&self) -> Result<bool, ParseError> {
        let mut lexer = self.lexer.clone();
        lexer.next_token(LexicalGoal::RegExp)?;
        Ok(lexer.next_token(LexicalGoal::Div)?.text == ":")
    }

    fn labeled(&mut self) -> Result<Stmt, ParseError> {
        let token = self.take(LexicalGoal::RegExp)?;
        self.literal_or_identifier(token)?;
        let label = token.identifier_name().unwrap().into_owned();
        if self.labels.iter().any(|(name, _)| *name == label) {
            return Err(error(token.span, "duplicate active label"));
        }
        self.expect(":")?;
        let mut lexer = self.lexer.clone();
        let iteration = loop {
            let next = lexer.next_token(LexicalGoal::RegExp)?;
            if matches!(next.text, "for" | "while" | "do") {
                break true;
            }
            if next.kind != TokenKind::Identifier || lexer.next_token(LexicalGoal::Div)?.text != ":"
            {
                break false;
            }
        };
        self.labels.push((label.clone(), iteration));
        let result = self.statement();
        self.labels.pop();
        Ok(self.stmt(
            token.span.start,
            StmtKind::Labeled {
                label,
                body: Box::new(result?),
            },
        ))
    }

    fn is_lexical_declaration(&self) -> Result<bool, ParseError> {
        let mut lexer = self.lexer.clone();
        lexer.next_token(LexicalGoal::Div)?;
        let next = lexer.next_token(LexicalGoal::Div)?;
        Ok(next.kind == TokenKind::Identifier || matches!(next.text, "[" | "{"))
    }

    pub(super) fn binding_identifier(&mut self) -> Result<Expr, ParseError> {
        let token = self.take(LexicalGoal::Div)?;
        if token.kind != TokenKind::Identifier {
            return Err(error(token.span, "expected binding identifier"));
        }
        let expr = self.literal_or_identifier(token)?;
        if self.strict()
            && matches!(&expr.kind, ExprKind::Identifier(name) if matches!(name.as_str(), "eval" | "arguments"))
        {
            return Err(error(expr.span, "invalid strict binding"));
        }
        Ok(expr)
    }

    fn variables(
        &mut self,
        for_header: bool,
    ) -> Result<(VariableKind, Vec<VariableDeclarator>), ParseError> {
        let token = self.take(LexicalGoal::Div)?;
        let kind = match token.text {
            "var" => VariableKind::Var,
            "let" => VariableKind::Let,
            _ => VariableKind::Const,
        };
        let mut declarations = vec![];
        loop {
            let binding = self.binding()?;
            if kind != VariableKind::Var
                && functions::bound_names(&binding)
                    .iter()
                    .any(|(name, _)| *name == "let")
            {
                return Err(error(binding.span, "let cannot be a lexical binding"));
            }
            let init = if self.at("=")? {
                self.expect("=")?;
                Some(self.expression_with_in(2, !for_header)?)
            } else {
                None
            };
            if !for_header
                && (kind == VariableKind::Const || !matches!(binding.kind, ExprKind::Identifier(_)))
                && init.is_none()
            {
                return Err(error(binding.span, "const requires an initializer"));
            }
            declarations.push(VariableDeclarator {
                span: Span {
                    start: binding.span.start,
                    end: self.last_end,
                },
                binding,
                init,
            });
            if !self.at(",")? {
                break;
            }
            self.expect(",")?;
        }
        Ok((kind, declarations))
    }

    fn for_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect("for")?.span.start;
        let asynchronous = if self.at("await")? {
            let token = self.expect("await")?;
            if !self.in_async {
                return Err(error(token.span, "for-await requires an async context"));
            }
            true
        } else {
            false
        };
        self.expect("(")?;
        let iteration_start = self.peek(LexicalGoal::Div)?;
        let init = if self.at(";")? {
            None
        } else {
            let token = self.peek(LexicalGoal::Div)?;
            Some(
                if matches!(token.text, "var" | "const")
                    || (token.text == "let" && self.is_lexical_declaration()?)
                {
                    let (kind, declarations) = self.variables(true)?;
                    ForInit::Variable { kind, declarations }
                } else {
                    ForInit::Expression(self.expression_with_in(0, false)?)
                },
            )
        };
        let next = self.peek(LexicalGoal::Div)?;
        if matches!(next.text, "in" | "of") {
            self.take(LexicalGoal::Div)?;
            let of = next.text == "of";
            if asynchronous && !of {
                return Err(error(next.span, "for-await requires of"));
            }
            let Some(left) = init else {
                return Err(error(next.span, "missing iteration target"));
            };
            match &left {
                ForInit::Expression(expr)
                    if !assignment_pattern(expr)
                        || (of
                            && (iteration_start.text == "let"
                                || (!asynchronous
                                    && iteration_start.text == "async"
                                    && matches!(&expr.kind, ExprKind::Identifier(_))))) =>
                {
                    return Err(error(expr.span, "invalid iteration target"));
                }
                ForInit::Variable { kind, declarations }
                    if declarations.len() != 1
                        || (declarations[0].init.is_some()
                            && (of
                                || *kind != VariableKind::Var
                                || self.strict()
                                || !matches!(
                                    declarations[0].binding.kind,
                                    ExprKind::Identifier(_)
                                ))) =>
                {
                    return Err(error(next.span, "invalid iteration declaration"));
                }
                _ => {}
            }
            let right = self.expression_with_in(if of { 2 } else { 0 }, true)?;
            self.expect(")")?;
            let body = self.loop_body()?;
            return Ok(self.stmt(
                start,
                StmtKind::ForEach {
                    left,
                    right,
                    of,
                    asynchronous,
                    body,
                },
            ));
        }
        if asynchronous {
            return Err(error(next.span, "for-await requires of"));
        }
        if let Some(ForInit::Variable { kind, declarations }) = &init {
            if let Some(declaration) = declarations.iter().find(|d| {
                d.init.is_none()
                    && (*kind == VariableKind::Const
                        || !matches!(d.binding.kind, ExprKind::Identifier(_)))
            }) {
                return Err(error(declaration.span, "const requires an initializer"));
            }
        }
        self.expect(";")?;
        let test = if self.at(";")? {
            None
        } else {
            Some(self.expression_with_in(0, true)?)
        };
        self.expect(";")?;
        let update = if self.at(")")? {
            None
        } else {
            Some(self.expression_with_in(0, true)?)
        };
        self.expect(")")?;
        let body = self.loop_body()?;
        Ok(self.stmt(
            start,
            StmtKind::For {
                init,
                test,
                update,
                body,
            },
        ))
    }

    fn switch_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect("switch")?.span.start;
        let discriminant = self.condition()?;
        self.expect("{")?;
        self.switches += 1;
        let result = self.switch_cases();
        self.switches -= 1;
        let cases = result?;
        self.expect("}")?;
        Ok(self.stmt(
            start,
            StmtKind::Switch {
                discriminant,
                cases,
            },
        ))
    }

    fn switch_cases(&mut self) -> Result<Vec<SwitchCase>, ParseError> {
        let mut cases = vec![];
        let mut default = false;
        while !self.at("}")? {
            let token = self.take(LexicalGoal::Div)?;
            let test = match token.text {
                "case" => Some(self.expression(0)?),
                "default" if !default => {
                    default = true;
                    None
                }
                _ => return Err(error(token.span, "expected case or single default clause")),
            };
            self.expect(":")?;
            let mut consequent = vec![];
            while !matches!(
                self.peek(LexicalGoal::RegExp)?.text,
                "case" | "default" | "}"
            ) {
                consequent.push(self.statement_list_item()?);
            }
            cases.push(SwitchCase {
                span: Span {
                    start: token.span.start,
                    end: self.last_end,
                },
                test,
                consequent,
            });
        }
        Ok(cases)
    }

    fn try_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect("try")?.span.start;
        let block = Box::new(self.block()?);
        let handler = if self.at("catch")? {
            let start = self.expect("catch")?.span.start;
            let param = if self.at("(")? {
                self.expect("(")?;
                let binding = self.binding()?;
                self.expect(")")?;
                Some(binding)
            } else {
                None
            };
            let body = Box::new(self.block()?);
            Some(CatchClause {
                span: Span {
                    start,
                    end: self.last_end,
                },
                param,
                body,
            })
        } else {
            None
        };
        let finalizer = if self.at("finally")? {
            self.expect("finally")?;
            Some(Box::new(self.block()?))
        } else {
            None
        };
        if handler.is_none() && finalizer.is_none() {
            return Err(error(block.span, "try requires catch or finally"));
        }
        Ok(self.stmt(
            start,
            StmtKind::Try {
                block,
                handler,
                finalizer,
            },
        ))
    }
}
