use super::*;
use crate::ast::{
    ExportDeclaration, ExportKind, ExportSpecifier, ImportAttribute, ImportDeclaration, ImportName,
    ImportSpecifier, ModuleRequest, Stmt, StmtKind,
};
use std::collections::HashSet;

/// Parses complete Module source with strict mode and top-level await enabled.
/// Static requests are available through `Program::module_requests`.
pub fn parse_module(source: &str) -> Result<crate::ast::Program, ParseError> {
    parse_program(
        source,
        ParserOptions {
            lexer: LexerOptions {
                source_goal: SourceGoal::Module,
                html_comments: false,
            },
            ..ParserOptions::default()
        },
    )
}

impl<'a> Parser<'a> {
    pub(super) fn module_item(&mut self) -> Result<Stmt, ParseError> {
        if self.options.lexer.source_goal == SourceGoal::Module && !self.in_function {
            let token = self.peek(LexicalGoal::RegExp)?;
            if token.text == "export" {
                return self.export_declaration();
            }
            if token.text == "import" {
                let mut lexer = self.lexer.clone();
                lexer.next_token(LexicalGoal::Div)?;
                if !matches!(lexer.next_token(LexicalGoal::Div)?.text, "(" | ".") {
                    return self.import_declaration();
                }
            }
        }
        self.statement_list_item()
    }

    fn module_string(&mut self) -> Result<(Vec<u16>, Span), ParseError> {
        let token = self.take(LexicalGoal::Div)?;
        if token.kind != TokenKind::String {
            return Err(error(token.span, "expected module string literal"));
        }
        Ok((token.string_value().unwrap(), token.span))
    }

    fn module_name(&mut self) -> Result<(Vec<u16>, Token<'a>), ParseError> {
        let token = self.take(LexicalGoal::Div)?;
        let name = if token.kind == TokenKind::String {
            let name = token.string_value().unwrap();
            if String::from_utf16(&name).is_err() {
                return Err(error(
                    token.span,
                    "module export name contains a lone surrogate",
                ));
            }
            name
        } else if matches!(
            token.kind,
            TokenKind::Identifier | TokenKind::Keyword | TokenKind::EscapedKeyword
        ) {
            token.identifier_name().unwrap().encode_utf16().collect()
        } else {
            return Err(error(token.span, "expected module export name"));
        };
        Ok((name, token))
    }

    fn module_request(&mut self) -> Result<ModuleRequest, ParseError> {
        let (specifier, span) = self.module_string()?;
        let mut attributes = vec![];
        let mut keys = HashSet::new();
        if self.at("with")? {
            self.expect("with")?;
            self.expect("{")?;
            while !self.at("}")? {
                let token = self.peek(LexicalGoal::Div)?;
                let key = if token.kind == TokenKind::String {
                    self.module_string()?.0
                } else if matches!(
                    token.kind,
                    TokenKind::Identifier | TokenKind::Keyword | TokenKind::EscapedKeyword
                ) {
                    self.take(LexicalGoal::Div)?;
                    token.identifier_name().unwrap().encode_utf16().collect()
                } else {
                    return Err(error(token.span, "expected import attribute key"));
                };
                if !keys.insert(key.clone()) {
                    return Err(error(token.span, "duplicate import attribute key"));
                }
                self.expect(":")?;
                let (value, _) = self.module_string()?;
                attributes.push(ImportAttribute {
                    span: Span {
                        start: token.span.start,
                        end: self.last_end,
                    },
                    key,
                    value,
                });
                if !self.at(",")? {
                    break;
                }
                self.expect(",")?;
            }
            self.expect("}")?;
        }
        Ok(ModuleRequest {
            span: Span {
                start: span.start,
                end: self.last_end,
            },
            specifier,
            attributes,
        })
    }

    fn import_declaration(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect("import")?.span.start;
        let mut specifiers = vec![];
        let request = if self.peek(LexicalGoal::Div)?.kind == TokenKind::String {
            self.module_request()?
        } else {
            let mut clause = true;
            if self.peek(LexicalGoal::Div)?.kind == TokenKind::Identifier {
                let local = self.binding_identifier()?;
                specifiers.push(ImportSpecifier {
                    span: local.span,
                    imported: ImportName::Name("default".encode_utf16().collect()),
                    local,
                });
                if self.at(",")? {
                    self.expect(",")?;
                } else {
                    clause = false;
                }
            }
            if clause {
                if self.at("*")? {
                    let start = self.expect("*")?.span.start;
                    self.expect("as")?;
                    let local = self.binding_identifier()?;
                    specifiers.push(ImportSpecifier {
                        span: Span {
                            start,
                            end: self.last_end,
                        },
                        imported: ImportName::Namespace,
                        local,
                    });
                } else {
                    self.expect("{")?;
                    while !self.at("}")? {
                        let (imported, first) = self.module_name()?;
                        let local = if self.at("as")? {
                            self.expect("as")?;
                            self.binding_identifier()?
                        } else {
                            if first.kind != TokenKind::Identifier {
                                return Err(error(
                                    first.span,
                                    "imported name requires a local alias",
                                ));
                            }
                            self.literal_or_identifier(first)?
                        };
                        specifiers.push(ImportSpecifier {
                            span: Span {
                                start: first.span.start,
                                end: self.last_end,
                            },
                            imported: ImportName::Name(imported),
                            local,
                        });
                        if !self.at(",")? {
                            break;
                        }
                        self.expect(",")?;
                    }
                    self.expect("}")?;
                }
            }
            self.expect("from")?;
            self.module_request()?
        };
        let mut locals = HashSet::new();
        for specifier in &specifiers {
            if let ExprKind::Identifier(name) = &specifier.local.kind {
                if !locals.insert(name) {
                    return Err(error(specifier.local.span, "duplicate import binding"));
                }
            }
        }
        self.semicolon()?;
        let span = Span {
            start,
            end: self.last_end,
        };
        Ok(Stmt {
            span,
            kind: StmtKind::ImportDeclaration(Box::new(ImportDeclaration {
                span,
                request,
                specifiers,
            })),
        })
    }

    fn export_declaration(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect("export")?.span.start;
        let kind = if self.at("*")? {
            self.expect("*")?;
            let exported = if self.at("as")? {
                self.expect("as")?;
                Some(self.module_name()?.0)
            } else {
                None
            };
            self.expect("from")?;
            let request = self.module_request()?;
            self.semicolon()?;
            ExportKind::All { exported, request }
        } else if self.at("{")? {
            self.expect("{")?;
            let mut specifiers = vec![];
            let mut local_tokens = vec![];
            while !self.at("}")? {
                let (local, first) = self.module_name()?;
                let exported = if self.at("as")? {
                    self.expect("as")?;
                    self.module_name()?.0
                } else {
                    local.clone()
                };
                specifiers.push(ExportSpecifier {
                    span: Span {
                        start: first.span.start,
                        end: self.last_end,
                    },
                    local_span: first.span,
                    local,
                    exported,
                });
                local_tokens.push(first);
                if !self.at(",")? {
                    break;
                }
                self.expect(",")?;
            }
            self.expect("}")?;
            let request = if self.at("from")? {
                self.expect("from")?;
                Some(self.module_request()?)
            } else {
                None
            };
            if request.is_none() {
                for token in local_tokens {
                    if token.kind != TokenKind::Identifier {
                        return Err(error(
                            token.span,
                            "local export requires an identifier reference",
                        ));
                    }
                    self.literal_or_identifier(token)?;
                }
            }
            self.semicolon()?;
            ExportKind::Named {
                specifiers,
                request,
            }
        } else if self.at("default")? {
            self.expect("default")?;
            let token = self.peek(LexicalGoal::RegExp)?;
            if token.text == "function" || self.is_async_function()? {
                let asynchronous = token.text == "async";
                if asynchronous {
                    self.expect("async")?;
                }
                let function = self.function(false, asynchronous, token.span.start)?;
                ExportKind::DefaultDeclaration(Box::new(Stmt {
                    span: function.span,
                    kind: StmtKind::FunctionDeclaration(Box::new(function)),
                }))
            } else if token.text == "class" {
                let class = self.class(false)?;
                ExportKind::DefaultDeclaration(Box::new(Stmt {
                    span: class.span,
                    kind: StmtKind::ClassDeclaration(Box::new(class)),
                }))
            } else {
                let expr = self.expression(2)?;
                self.semicolon()?;
                ExportKind::DefaultExpression(expr)
            }
        } else {
            let token = self.peek(LexicalGoal::RegExp)?;
            if !matches!(token.text, "var" | "let" | "const" | "function" | "class")
                && !self.is_async_function()?
            {
                return Err(error(token.span, "expected exported declaration"));
            }
            let stmt = self.statement_list_item()?;
            if !matches!(
                stmt.kind,
                StmtKind::Variable { .. }
                    | StmtKind::FunctionDeclaration(_)
                    | StmtKind::ClassDeclaration(_)
            ) {
                return Err(error(stmt.span, "expected exported declaration"));
            }
            ExportKind::Declaration(Box::new(stmt))
        };
        let span = Span {
            start,
            end: self.last_end,
        };
        Ok(Stmt {
            span,
            kind: StmtKind::ExportDeclaration(Box::new(ExportDeclaration { span, kind })),
        })
    }
}
