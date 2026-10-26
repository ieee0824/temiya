use super::*;

impl Parser<'_> {
    pub(super) fn definition_key(&mut self, private: bool) -> Result<(Expr, bool), ParseError> {
        if self.at("[")? {
            self.expect("[")?;
            let key = self.expression_with_in(2, true)?;
            self.expect("]")?;
            return Ok((key, true));
        }
        let token = self.peek(LexicalGoal::Div)?;
        if token.kind == TokenKind::PrivateIdentifier && !private {
            return Err(error(token.span, "private name outside a class element"));
        }
        if token.identifier_name().is_some() {
            return Ok((self.property_name()?, false));
        }
        if !matches!(
            token.kind,
            TokenKind::String
                | TokenKind::LegacyString
                | TokenKind::Number
                | TokenKind::BigInt
                | TokenKind::LegacyOctalNumber
                | TokenKind::LegacyDecimalNumber
        ) {
            return Err(error(token.span, "expected property definition name"));
        }
        self.take(LexicalGoal::Div)?;
        Ok((self.literal_or_identifier(token)?, false))
    }
}

impl Parser<'_> {
    pub(super) fn method_prefix(&mut self) -> Result<(bool, bool, MethodKind), ParseError> {
        let first = self.peek(LexicalGoal::Div)?;
        let mut asynchronous = false;
        let mut kind = MethodKind::Method;
        if matches!(first.text, "async" | "get" | "set") {
            let mut lexer = self.lexer.clone();
            lexer.next_token(LexicalGoal::Div)?;
            let next = lexer.next_token(LexicalGoal::Div)?;
            if !matches!(next.text, "(" | "=" | ";" | "}" | ":" | ",")
                && (first.text != "async" || !next.has_line_terminator_before)
            {
                self.take(LexicalGoal::Div)?;
                match first.text {
                    "async" => asynchronous = true,
                    "get" => kind = MethodKind::Get,
                    _ => kind = MethodKind::Set,
                }
            }
        }
        let generator = self.at("*")?;
        if generator {
            self.expect("*")?;
        }
        if generator && kind != MethodKind::Method {
            return Err(error(first.span, "accessor cannot be a generator"));
        }
        Ok((asynchronous, generator, kind))
    }

    pub(super) fn method(
        &mut self,
        start: usize,
        asynchronous: bool,
        generator: bool,
        kind: MethodKind,
        derived_constructor: bool,
    ) -> Result<Function, ParseError> {
        let mut nested = self.nested_function(asynchronous, generator, false);
        nested.super_property = true;
        nested.super_call = derived_constructor;
        let parameters = nested.parameters()?;
        if (kind == MethodKind::Get && !parameters.is_empty())
            || (kind == MethodKind::Set
                && (parameters.len() != 1 || matches!(parameters[0].kind, ExprKind::Spread(_))))
        {
            return Err(error(
                Span {
                    start,
                    end: nested.last_end,
                },
                "invalid accessor parameters",
            ));
        }
        let (block, directive) = nested.function_block()?;
        nested.check_parameters(&parameters, nested.strict(), true, directive)?;
        let function = Function {
            span: Span {
                start,
                end: nested.last_end,
            },
            name: None,
            parameters,
            body: FunctionBody::Block(Box::new(block)),
            asynchronous,
            generator,
            strict: nested.strict(),
        };
        self.commit_cursor(nested);
        Ok(function)
    }

    pub(super) fn class(&mut self, declaration: bool) -> Result<Class, ParseError> {
        let start = self.expect("class")?.span.start;
        let mut nested = self.clone();
        nested.options.strict = true;
        nested.class_depth += 1;
        let name = if nested.peek(LexicalGoal::Div)?.kind == TokenKind::Identifier {
            Some(nested.binding_identifier()?)
        } else if declaration {
            return Err(error(
                nested.peek(LexicalGoal::Div)?.span,
                "class declaration requires a name",
            ));
        } else {
            None
        };
        let super_class = if nested.at("extends")? {
            nested.expect("extends")?;
            Some(nested.expression(19)?)
        } else {
            None
        };
        nested.expect("{")?;
        let mut elements = vec![];
        let mut constructor = false;
        let mut private_names: std::collections::HashMap<
            String,
            (bool, Option<MethodKind>, usize),
        > = std::collections::HashMap::new();
        while !nested.at("}")? {
            if nested.at(";")? {
                nested.expect(";")?;
                continue;
            }
            let first = nested.peek(LexicalGoal::Div)?;
            let mut is_static = false;
            if first.text == "static" {
                let mut lexer = nested.lexer.clone();
                lexer.next_token(LexicalGoal::Div)?;
                let next = lexer.next_token(LexicalGoal::Div)?;
                if next.text == "{" {
                    nested.expect("static")?;
                    let mut block_parser = nested.field_context(true);
                    let block = block_parser.block()?;
                    nested.commit_cursor(block_parser);
                    elements.push(ClassElement {
                        span: Span {
                            start: first.span.start,
                            end: nested.last_end,
                        },
                        key: None,
                        computed: false,
                        is_static: true,
                        kind: ClassElementKind::StaticBlock(Box::new(block)),
                    });
                    continue;
                }
                if !matches!(next.text, "(" | "=" | ";" | "}") {
                    nested.expect("static")?;
                    is_static = true;
                }
            }
            let (asynchronous, generator, mut method_kind) = nested.method_prefix()?;
            let (key, computed) = nested.definition_key(true)?;
            let private_name = if let ExprKind::PrivateIdentifier(name) = &key.kind {
                Some(name.clone())
            } else {
                None
            };
            let public_name = if !computed {
                property_string(&key)
            } else {
                None
            };
            if private_name.as_deref() == Some("constructor") {
                return Err(error(key.span, "private constructor name is forbidden"));
            }
            if is_static && public_name.as_deref() == Some("prototype") {
                return Err(error(key.span, "static prototype name is forbidden"));
            }
            let kind = if nested.at("(")? {
                if !is_static && public_name.as_deref() == Some("constructor") {
                    if constructor || asynchronous || generator || method_kind != MethodKind::Method
                    {
                        return Err(error(key.span, "invalid or duplicate constructor"));
                    }
                    constructor = true;
                    method_kind = MethodKind::Constructor;
                }
                let function = nested.method(
                    first.span.start,
                    asynchronous,
                    generator,
                    method_kind,
                    method_kind == MethodKind::Constructor && super_class.is_some(),
                )?;
                ClassElementKind::Method {
                    function: Box::new(function),
                    kind: method_kind,
                }
            } else {
                if asynchronous || generator || method_kind != MethodKind::Method {
                    return Err(error(key.span, "method modifier requires a method"));
                }
                if public_name.as_deref() == Some("constructor") {
                    return Err(error(key.span, "constructor cannot be a field"));
                }
                let initializer = if nested.at("=")? {
                    nested.expect("=")?;
                    let mut field_parser = nested.field_context(false);
                    let expr = field_parser.expression(2)?;
                    nested.commit_cursor(field_parser);
                    Some(expr)
                } else {
                    None
                };
                nested.semicolon()?;
                ClassElementKind::Field(initializer)
            };
            if let Some(name) = private_name {
                let new_kind = if let ClassElementKind::Method { kind, .. } = &kind {
                    Some(*kind)
                } else {
                    None
                };
                if let Some((previous_static, previous_kind, count)) = private_names.get_mut(&name)
                {
                    let pair = matches!(
                        (*previous_kind, new_kind),
                        (Some(MethodKind::Get), Some(MethodKind::Set))
                            | (Some(MethodKind::Set), Some(MethodKind::Get))
                    );
                    if *previous_static != is_static || *count != 1 || !pair {
                        return Err(error(key.span, "duplicate private name"));
                    }
                    *count += 1;
                } else {
                    private_names.insert(name, (is_static, new_kind, 1));
                }
            }
            elements.push(ClassElement {
                span: Span {
                    start: first.span.start,
                    end: nested.last_end,
                },
                key: Some(key),
                computed,
                is_static,
                kind,
            });
        }
        nested.expect("}")?;
        let class = Class {
            span: Span {
                start,
                end: nested.last_end,
            },
            name,
            super_class,
            elements,
        };
        self.commit_cursor(nested);
        Ok(class)
    }

    fn field_context(&self, static_block: bool) -> Self {
        let mut nested = self.clone();
        nested.in_function = false;
        nested.in_async = false;
        nested.in_generator = false;
        nested.in_parameters = false;
        nested.allow_in = true;
        nested.loops = 0;
        nested.switches = 0;
        nested.labels.clear();
        nested.super_property = true;
        nested.super_call = false;
        nested.new_target = true;
        nested.forbid_arguments = true;
        nested.await_reserved =
            static_block || self.options.lexer.source_goal == SourceGoal::Module;
        nested
    }
}

pub(super) fn property_string(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name.clone()),
        ExprKind::Literal(Literal::String { value, .. }) => String::from_utf16(value).ok(),
        _ => None,
    }
}
