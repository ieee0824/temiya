use super::*;

pub(super) struct ControlTarget {
    labels: Vec<String>,
    iteration: bool,
    scope_depth: usize,
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

impl Compiler {
    pub(super) fn reset_completion(&mut self) -> Result<(), CompileError> {
        if self.track_completion {
            self.constant(Constant::Undefined)?;
            self.code
                .push(Instruction::StoreLocal(self.completion_local));
        }
        Ok(())
    }

    fn push_target(&mut self, labels: Vec<String>, iteration: bool) {
        self.targets.push(ControlTarget {
            labels,
            iteration,
            scope_depth: self.scopes.len(),
            breaks: vec![],
            continues: vec![],
        });
    }

    pub(super) fn abrupt_jump(
        &mut self,
        label: Option<&str>,
        continuing: bool,
        span: Span,
    ) -> Result<bool, CompileError> {
        let target = self
            .targets
            .iter()
            .rposition(|target| match label {
                Some(label) => target.labels.iter().any(|name| name == label),
                None => target.iteration,
            })
            .ok_or_else(|| self.unsupported("unbound break/continue target", span))?;
        if continuing && !self.targets[target].iteration {
            return Err(self.unsupported("continue to a non-iteration label", span));
        }
        for _ in self.targets[target].scope_depth..self.scopes.len() {
            self.code.push(Instruction::ExitScope);
        }
        let jump = self.emit(Instruction::Jump(0));
        if continuing {
            self.targets[target].continues.push(jump);
        } else {
            self.targets[target].breaks.push(jump);
        }
        Ok(true)
    }

    pub(super) fn labeled_statement(&mut self, statement: &Stmt) -> Result<bool, CompileError> {
        let mut labels = vec![];
        let mut body = statement;
        while let StmtKind::Labeled { label, body: next } = &body.kind {
            if labels.contains(label) || self.targets.iter().any(|t| t.labels.contains(label)) {
                return Err(self.unsupported("duplicate active label", body.span));
            }
            labels.push(label.clone());
            body = next;
        }
        if matches!(
            body.kind,
            StmtKind::For { .. } | StmtKind::While { .. } | StmtKind::DoWhile { .. }
        ) {
            return self.loop_statement(body, labels);
        }
        self.push_target(labels, false);
        let terminal = self.statement(body)?;
        let target = self.targets.pop().expect("label target");
        let end = self.offset()?;
        for jump in &target.breaks {
            self.patch(*jump, end);
        }
        Ok(terminal && target.breaks.is_empty())
    }

    fn loop_initializer(
        &mut self,
        init: Option<&ForInit>,
        span: Span,
    ) -> Result<(bool, bool), CompileError> {
        let mut lexical = false;
        let mut per_iteration = false;
        match init {
            Some(ForInit::Variable { kind, declarations }) => {
                if *kind != VariableKind::Var {
                    let bindings = declarations
                        .iter()
                        .map(|declaration| {
                            let ExprKind::Identifier(name) = &declaration.binding.kind else {
                                return Err(self
                                    .unsupported("for binding pattern", declaration.binding.span));
                            };
                            Ok(ScopeBinding {
                                name: JsString::from(name.as_str()),
                                kind: if *kind == VariableKind::Let {
                                    BindingKind::Let
                                } else {
                                    BindingKind::Const
                                },
                            })
                        })
                        .collect::<Result<Vec<_>, CompileError>>()?;
                    self.enter_scope(ScopeKind::Block, bindings)?;
                    lexical = true;
                    per_iteration = *kind == VariableKind::Let;
                }
                self.statement(&Stmt {
                    span,
                    kind: StmtKind::Variable {
                        kind: *kind,
                        declarations: declarations.clone(),
                    },
                })?;
            }
            Some(ForInit::Expression(expression)) => {
                self.expression(expression)?;
                self.code.push(Instruction::Pop);
            }
            None => {}
        }
        Ok((lexical, per_iteration))
    }

    pub(super) fn loop_statement(
        &mut self,
        statement: &Stmt,
        labels: Vec<String>,
    ) -> Result<bool, CompileError> {
        let (init, test, update, body, first) = match &statement.kind {
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => (
                init.as_ref(),
                test.as_ref(),
                update.as_ref(),
                body.as_ref(),
                false,
            ),
            StmtKind::While { test, body } => (None, Some(test), None, body.as_ref(), false),
            StmtKind::DoWhile { test, body } => (None, Some(test), None, body.as_ref(), true),
            _ => unreachable!(),
        };
        let (lexical, per_iteration) = self.loop_initializer(init, statement.span)?;
        self.reset_completion()?;
        if per_iteration {
            self.code.push(Instruction::CloneLoopBindings);
        }
        self.push_target(labels, true);
        let head = self.offset()?;
        let exit = if !first {
            if let Some(test) = test {
                self.expression(test)?;
                Some(self.emit(Instruction::JumpIfFalse(0)))
            } else {
                None
            }
        } else {
            None
        };
        let terminal = self.statement(body)?;
        let target = self.targets.pop().expect("loop target");
        let step_reachable = !terminal || !target.continues.is_empty();
        let step = self.offset()?;
        for jump in &target.continues {
            self.patch(*jump, step);
        }
        let mut exit = exit;
        if step_reachable {
            if per_iteration {
                self.code.push(Instruction::CloneLoopBindings);
            }
            if let Some(update) = update {
                self.expression(update)?;
                self.code.push(Instruction::Pop);
            }
            if first {
                self.expression(test.expect("do-while condition"))?;
                exit = Some(self.emit(Instruction::JumpIfFalse(0)));
            }
            self.code.push(Instruction::Jump(head));
        }
        let end = self.offset()?;
        if let Some(jump) = exit {
            self.patch(jump, end);
        }
        for jump in &target.breaks {
            self.patch(*jump, end);
        }
        let falls_through = exit.is_some() || !target.breaks.is_empty();
        if lexical {
            self.scopes.pop();
            if falls_through {
                self.code.push(Instruction::ExitScope);
            }
        }
        Ok(!falls_through)
    }
}
