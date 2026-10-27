use std::{collections::HashMap, fmt};

use crate::{
    ast::{
        Expr, ExprKind, ForInit, Function, FunctionBody, Literal, Program, Stmt, StmtKind,
        VariableKind,
    },
    lexer::Span,
    parser::parse_script,
    runtime::{JsString, Value, parse_numeric_literal},
};

mod control_flow;

use super::{
    BinaryOperator, BindingKind, Bytecode, Constant, FunctionTemplate, Instruction, ScopeBinding,
    ScopeDescriptor, ScopeFunction, ScopeKind, UnaryNumericOperator, VerifiedBytecode, VerifyError,
};

/// AST-to-bytecode compiler for the portable baseline instruction set.
#[derive(Debug, Default)]
pub struct BytecodeCompiler;

impl BytecodeCompiler {
    /// Parses Script source, lowers it, and verifies the resulting bytecode.
    pub fn compile(source: &str) -> Result<VerifiedBytecode, CompileError> {
        let program =
            parse_script(source).map_err(|error| CompileError::new(error.to_string(), None))?;
        Self::compile_program(&program)
    }

    /// Compiles a function body for a caller-prepared function Environment.
    /// Parameter instantiation, async and generators are not lowered here.
    pub fn compile_function_body(function: &Function) -> Result<VerifiedBytecode, CompileError> {
        if !function.parameters.is_empty() || function.asynchronous || function.generator {
            return Err(CompileError::new(
                "unsupported function parameters, async or generator",
                Some(function.span),
            ));
        }
        crate::semantics::validate_expression(
            &Expr {
                span: function.span,
                kind: ExprKind::Function(Box::new(function.clone())),
            },
            crate::semantics::SemanticOptions::default(),
        )
        .map_err(|error| CompileError::new(error.to_string(), Some(error.span)))?;
        let mut compiler = Compiler::empty(function.strict);
        match &function.body {
            FunctionBody::Block(body) => {
                let StmtKind::Block(statements) = &body.kind else {
                    return Err(CompileError::new(
                        "expected function block",
                        Some(body.span),
                    ));
                };
                let vars = scope_bindings(statements, true, false, true)?;
                compiler.enter_scope(ScopeKind::Function, vars)?;
                let target = compiler.scopes.last().expect("function scope").entry;
                let terminal = compiler.block_with_functions(statements, Some(target))?;
                if !terminal {
                    compiler.constant(Constant::Undefined)?;
                    compiler.code.push(Instruction::Return);
                }
            }
            FunctionBody::Expression(expression) => {
                compiler.enter_scope(ScopeKind::Function, vec![])?;
                compiler.expression(expression)?;
                compiler.code.push(Instruction::Return);
            }
        }
        compiler.finish()
    }

    /// Lowers an owned AST and verifies the resulting bytecode.
    pub fn compile_program(program: &Program) -> Result<VerifiedBytecode, CompileError> {
        crate::semantics::validate_program(program, crate::semantics::SemanticOptions::default())
            .map_err(|error| CompileError::new(error.to_string(), Some(error.span)))?;
        Compiler::new(program)?.compile(program)
    }
}

/// Compilation failure for syntax outside the baseline bytecode subset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileError {
    message: String,
    span: Option<Span>,
}

impl CompileError {
    fn new(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    /// Returns the compiler diagnostic.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the unsupported or invalid source span when available.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        self.span
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.span {
            Some(span) => write!(
                formatter,
                "{} at bytes {}..{}",
                self.message, span.start, span.end
            ),
            None => formatter.write_str(&self.message),
        }
    }
}

impl std::error::Error for CompileError {}

struct CompileScope {
    entry: u32,
    kind: ScopeKind,
    bindings: HashMap<String, (u32, BindingKind)>,
}

struct Compiler {
    constants: Vec<Constant>,
    code: Vec<Instruction>,
    scopes: Vec<CompileScope>,
    strict: bool,
    completion_local: u32,
    track_completion: bool,
    targets: Vec<control_flow::ControlTarget>,
}

impl Compiler {
    fn empty(strict: bool) -> Self {
        Self {
            constants: vec![],
            code: vec![],
            scopes: vec![],
            strict,
            completion_local: 0,
            track_completion: false,
            targets: vec![],
        }
    }

    fn new(program: &Program) -> Result<Self, CompileError> {
        let mut compiler = Self::empty(program.strict);
        compiler.track_completion = true;
        compiler.enter_scope(
            ScopeKind::Global,
            scope_bindings(&program.body, true, true, true)?,
        )?;
        compiler.attach_functions(&program.body, 0)?;
        Ok(compiler)
    }

    fn compile(mut self, program: &Program) -> Result<VerifiedBytecode, CompileError> {
        if !self.statements(&program.body)? {
            self.code
                .push(Instruction::LoadLocal(self.completion_local));
            self.code.push(Instruction::ExitScope);
            self.code.push(Instruction::Complete);
        }
        self.finish()
    }

    fn finish(self) -> Result<VerifiedBytecode, CompileError> {
        Bytecode {
            constants: self.constants,
            instructions: self.code,
            local_count: u32::from(self.track_completion),
        }
        .verify()
        .map_err(|error: VerifyError| CompileError::new(error.to_string(), None))
    }

    fn enter_scope(
        &mut self,
        kind: ScopeKind,
        bindings: Vec<ScopeBinding>,
    ) -> Result<(), CompileError> {
        let entry = self.offset()?;
        let mut slots = HashMap::new();
        for (index, binding) in bindings.iter().enumerate() {
            let slot = u32::try_from(index)
                .map_err(|_| CompileError::new("binding slot overflow", None))?;
            slots.insert(binding.name.to_string_lossy(), (slot, binding.kind));
        }
        let index = self.pool(Constant::Scope(ScopeDescriptor {
            kind,
            strict: self.strict,
            bindings,
            functions: vec![],
        }))?;
        self.code.push(Instruction::EnterScope(index));
        self.scopes.push(CompileScope {
            entry,
            kind,
            bindings: slots,
        });
        Ok(())
    }

    fn attach_functions(&mut self, statements: &[Stmt], target: u32) -> Result<(), CompileError> {
        let mut selected = vec![];
        let mut names = vec![];
        for statement in statements.iter().rev() {
            if let StmtKind::FunctionDeclaration(function) = &statement.kind {
                let Some(ExprKind::Identifier(name)) =
                    function.name.as_ref().map(|name| &name.kind)
                else {
                    return Err(self.unsupported("anonymous function declaration", function.span));
                };
                if !names.contains(name) {
                    names.push(name.clone());
                    let mut template = self.function_template(function, false)?;
                    template.self_binding = false;
                    selected.push(ScopeFunction {
                        name: JsString::from(name.as_str()),
                        target,
                        template: std::rc::Rc::new(template),
                    });
                }
            }
        }
        selected.reverse();
        let scope = self.scopes.last().expect("declaration scope").entry;
        let Instruction::EnterScope(index) = self.code[scope as usize] else {
            unreachable!()
        };
        let Constant::Scope(plan) = &mut self.constants[index as usize] else {
            unreachable!()
        };
        plan.functions = selected;
        Ok(())
    }

    fn block(&mut self, statements: &[Stmt]) -> Result<bool, CompileError> {
        self.block_with_functions(statements, None)
    }

    fn block_with_functions(
        &mut self,
        statements: &[Stmt],
        var_target: Option<u32>,
    ) -> Result<bool, CompileError> {
        let block_functions = var_target.is_none();
        let has_functions = statements
            .iter()
            .any(|statement| matches!(statement.kind, StmtKind::FunctionDeclaration(_)));
        if block_functions && has_functions && !self.strict {
            return Err(CompileError::new(
                "unsupported sloppy block function Annex B semantics",
                None,
            ));
        }
        let bindings = scope_bindings(statements, false, true, block_functions)?;
        if bindings.is_empty() && !has_functions {
            return self.statements(statements);
        }
        self.enter_scope(ScopeKind::Block, bindings)?;
        let target = var_target.unwrap_or_else(|| self.scopes.last().expect("block scope").entry);
        self.attach_functions(statements, target)?;
        let result = self.statements(statements);
        self.scopes.pop();
        if matches!(result, Ok(false)) {
            self.code.push(Instruction::ExitScope);
        }
        result
    }

    fn statements(&mut self, statements: &[Stmt]) -> Result<bool, CompileError> {
        for statement in statements {
            if matches!(statement.kind, StmtKind::FunctionDeclaration(_)) {
                continue;
            }
            if self.statement(statement)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn statement(&mut self, statement: &Stmt) -> Result<bool, CompileError> {
        match &statement.kind {
            StmtKind::Empty | StmtKind::Debugger => Ok(false),
            StmtKind::FunctionDeclaration(_) => Err(self.unsupported(
                "function declaration outside declaration list",
                statement.span,
            )),
            StmtKind::Expression(expression) => {
                self.expression(expression)?;
                self.code.push(if self.track_completion {
                    Instruction::StoreLocal(self.completion_local)
                } else {
                    Instruction::Pop
                });
                Ok(false)
            }
            StmtKind::Variable { kind, declarations } => {
                for declaration in declarations {
                    let ExprKind::Identifier(name) = &declaration.binding.kind else {
                        return Err(self.unsupported("binding pattern", declaration.binding.span));
                    };
                    let (scope, slot, _) = self
                        .binding(name)
                        .expect("declaration collected before lowering");
                    let global_var =
                        *kind == VariableKind::Var && self.local_binding(name).is_none();
                    if let Some(init) = &declaration.init {
                        if global_var {
                            let name = self.pool(Constant::String(name.as_str().into()))?;
                            self.code.push(Instruction::ResolveName {
                                name,
                                strict: self.strict,
                            });
                        }
                        self.expression(init)?;
                    } else if *kind == VariableKind::Var {
                        continue;
                    } else if *kind == VariableKind::Const {
                        return Err(self.unsupported("const without initializer", declaration.span));
                    } else {
                        self.constant(Constant::Undefined)?;
                    }
                    self.code.push(if global_var {
                        Instruction::StoreReference
                    } else if *kind == VariableKind::Var {
                        Instruction::StoreBinding { scope, slot }
                    } else {
                        Instruction::InitializeBinding { scope, slot }
                    });
                }
                Ok(false)
            }
            StmtKind::Block(body) => self.block(body),
            StmtKind::If {
                test,
                consequent,
                alternate,
            } => {
                self.reset_completion()?;
                self.expression(test)?;
                let false_jump = self.emit(Instruction::JumpIfFalse(0));
                let consequent_terminal = self.statement(consequent)?;
                let end_jump = (!consequent_terminal).then(|| self.emit(Instruction::Jump(0)));
                let alternate_start = self.offset()?;
                self.patch(false_jump, alternate_start);
                let alternate_terminal = if let Some(alternate) = alternate {
                    self.statement(alternate)?
                } else {
                    false
                };
                if let Some(end_jump) = end_jump {
                    let end = self.offset()?;
                    self.patch(end_jump, end);
                }
                Ok(alternate.is_some() && consequent_terminal && alternate_terminal)
            }
            StmtKind::While { .. } | StmtKind::DoWhile { .. } | StmtKind::For { .. } => {
                self.loop_statement(statement, vec![])
            }
            StmtKind::Labeled { .. } => self.labeled_statement(statement),
            StmtKind::Break(label) => self.abrupt_jump(label.as_deref(), false, statement.span),
            StmtKind::Continue(label) => self.abrupt_jump(label.as_deref(), true, statement.span),
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expression(value)?;
                } else {
                    self.constant(Constant::Undefined)?;
                }
                self.code.push(Instruction::Return);
                Ok(true)
            }
            StmtKind::Throw(value) => {
                self.expression(value)?;
                self.code.push(Instruction::Throw);
                Ok(true)
            }
            _ => Err(self.unsupported("statement", statement.span)),
        }
    }

    fn expression(&mut self, expression: &Expr) -> Result<(), CompileError> {
        match &expression.kind {
            ExprKind::Literal(literal) => self.literal(literal, expression.span),
            ExprKind::Array { elements, .. } => {
                self.code.push(Instruction::CreateArray);
                for element in elements {
                    match element {
                        Some(Expr {
                            kind: ExprKind::Spread(value),
                            ..
                        }) => {
                            self.expression(value)?;
                            self.code.push(Instruction::SpreadArray);
                        }
                        Some(value) => {
                            self.expression(value)?;
                            self.code.push(Instruction::AppendArray);
                        }
                        None => self.code.push(Instruction::ElideArray),
                    }
                }
                self.code.push(Instruction::FinishArray);
                Ok(())
            }
            ExprKind::Object(properties) => {
                self.code.push(Instruction::CreateObject);
                for property in properties {
                    if property.spread {
                        self.expression(&property.key)?;
                        self.code.push(Instruction::SpreadObject);
                        continue;
                    }
                    if matches!(
                        property.value.as_ref().map(|value| &value.kind),
                        Some(ExprKind::Method { .. })
                    ) {
                        return Err(self.unsupported("object method", property.span));
                    }
                    if property.computed
                        || matches!(property.key.kind, ExprKind::Literal(Literal::Number { .. }))
                    {
                        self.expression(&property.key)?;
                    } else {
                        self.property_key(&property.key, false)?;
                    }
                    self.code.push(Instruction::CoercePropertyKey);
                    self.expression(property.value.as_ref().unwrap_or(&property.key))?;
                    let proto_name = match &property.key.kind {
                        ExprKind::Identifier(name) => name == "__proto__",
                        ExprKind::Literal(Literal::String { value, .. }) => {
                            value.as_slice()
                                == "__proto__".encode_utf16().collect::<Vec<_>>().as_slice()
                        }
                        _ => false,
                    };
                    self.code.push(Instruction::DefineObjectProperty {
                        prototype_setter: !property.computed && !property.shorthand && proto_name,
                    });
                }
                Ok(())
            }
            ExprKind::Identifier(name) => {
                self.load_name(name)?;
                Ok(())
            }
            ExprKind::Function(function) | ExprKind::Arrow(function) => {
                let arrow = matches!(expression.kind, ExprKind::Arrow(_));
                let template = self.function_template(function, arrow)?;
                let index = self.pool(Constant::Function(std::rc::Rc::new(template)))?;
                self.code.push(Instruction::CreateFunction(index));
                Ok(())
            }
            ExprKind::MetaProperty { meta, property } if meta == "new" && property == "target" => {
                if !self.constants.iter().any(|constant| matches!(constant, Constant::Scope(plan) if plan.kind == ScopeKind::Function)) {
                    return Err(self.unsupported("new.target outside function", expression.span));
                }
                self.code.push(Instruction::LoadNewTarget);
                Ok(())
            }
            ExprKind::Call { .. } => self.chain_value(expression, ChainUse::Value),
            ExprKind::New { callee, arguments } => self.construct(callee, arguments),
            ExprKind::This => {
                self.code.push(Instruction::LoadThis);
                Ok(())
            }
            ExprKind::Member { .. } => self.member_value(expression, false),
            ExprKind::Parenthesized(inner) => self.expression(inner),
            ExprKind::Assignment {
                operator,
                target,
                value,
            } if operator == "=" => {
                let target = unparenthesized(target);
                let binding = match &target.kind {
                    ExprKind::Identifier(name) => {
                        let binding = self.local_binding(name);
                        if binding.is_none() {
                            let name = self.pool(Constant::String(name.as_str().into()))?;
                            self.code.push(Instruction::ResolveName {
                                name,
                                strict: self.strict,
                            });
                        }
                        binding
                    }
                    ExprKind::Member {
                        object,
                        property,
                        computed,
                        optional: false,
                    } => {
                        self.expression(object)?;
                        self.property_key(property, *computed)?;
                        self.code.push(Instruction::ResolveProperty {
                            strict: self.strict,
                        });
                        None
                    }
                    _ => return Err(self.unsupported("assignment target", target.span)),
                };
                self.expression(value)?;
                self.code.push(Instruction::Duplicate);
                self.code.push(if let Some((scope, slot, _)) = binding {
                    Instruction::StoreBinding { scope, slot }
                } else {
                    Instruction::StoreReference
                });
                Ok(())
            }
            ExprKind::Assignment {
                operator,
                target,
                value,
            } => self.compound_assignment(operator, target, value, expression.span),
            ExprKind::Binary {
                operator,
                left,
                right,
            } => {
                self.expression(left)?;
                if matches!(operator.as_str(), "&&" | "||" | "??") {
                    self.code.push(Instruction::Duplicate);
                    if operator == "??" {
                        let rhs = self.emit(Instruction::JumpIfNullish(0));
                        let end = self.emit(Instruction::Jump(0));
                        let start = self.offset()?;
                        self.patch(rhs, start);
                        self.code.push(Instruction::Pop);
                        self.expression(right)?;
                        let finish = self.offset()?;
                        self.patch(end, finish);
                    } else {
                        if operator == "||" {
                            self.code.push(Instruction::LogicalNot);
                        }
                        let end = self.emit(Instruction::JumpIfFalse(0));
                        self.code.push(Instruction::Pop);
                        self.expression(right)?;
                        let finish = self.offset()?;
                        self.patch(end, finish);
                    }
                } else {
                    self.expression(right)?;
                    self.emit_binary(operator, expression.span)?;
                }
                Ok(())
            }
            ExprKind::Update {
                operator,
                argument,
                prefix,
            } => {
                if !matches!(operator.as_str(), "++" | "--") {
                    return Err(self.unsupported("update operator", expression.span));
                }
                self.resolve_update_reference(argument)?;
                self.code.push(Instruction::UpdateReference {
                    increment: operator == "++",
                    prefix: *prefix,
                });
                Ok(())
            }
            ExprKind::Unary { operator, argument }
                if matches!(operator.as_str(), "+" | "-" | "~") =>
            {
                self.expression(argument)?;
                self.code
                    .push(Instruction::UnaryNumeric(match operator.as_str() {
                        "+" => UnaryNumericOperator::Plus,
                        "-" => UnaryNumericOperator::Minus,
                        _ => UnaryNumericOperator::BitNot,
                    }));
                Ok(())
            }
            ExprKind::Unary { operator, argument } if operator == "void" => {
                self.expression(argument)?;
                self.code.push(Instruction::Pop);
                self.constant(Constant::Undefined)
            }
            ExprKind::Unary { operator, argument } if operator == "typeof" => {
                if let ExprKind::Identifier(name) = &unparenthesized(argument).kind {
                    let name = self.pool(Constant::String(name.as_str().into()))?;
                    self.code.push(Instruction::ResolveName {
                        name,
                        strict: self.strict,
                    });
                    self.code.push(Instruction::TypeOfReference);
                } else {
                    self.expression(argument)?;
                    self.code.push(Instruction::TypeOf);
                }
                Ok(())
            }
            ExprKind::Unary { operator, argument } if operator == "delete" => {
                let argument = unparenthesized(argument);
                match &argument.kind {
                    ExprKind::Member { .. } => self.member_value(argument, true),
                    ExprKind::Identifier(name) => {
                        let name = self.pool(Constant::String(name.as_str().into()))?;
                        self.code.push(Instruction::ResolveName {
                            name,
                            strict: self.strict,
                        });
                        self.code.push(Instruction::DeleteReference);
                        Ok(())
                    }
                    _ => {
                        self.expression(argument)?;
                        self.code.push(Instruction::Pop);
                        self.constant(Constant::Boolean(true))
                    }
                }
            }
            ExprKind::Unary { operator, argument } if operator == "!" => {
                self.expression(argument)?;
                self.code.push(Instruction::LogicalNot);
                Ok(())
            }
            ExprKind::Conditional {
                test,
                consequent,
                alternate,
            } => {
                self.expression(test)?;
                let otherwise = self.emit(Instruction::JumpIfFalse(0));
                self.expression(consequent)?;
                let end_jump = self.emit(Instruction::Jump(0));
                let alternate_start = self.offset()?;
                self.patch(otherwise, alternate_start);
                self.expression(alternate)?;
                let end = self.offset()?;
                self.patch(end_jump, end);
                Ok(())
            }
            ExprKind::Sequence(expressions) if !expressions.is_empty() => {
                for (index, item) in expressions.iter().enumerate() {
                    self.expression(item)?;
                    if index + 1 != expressions.len() {
                        self.code.push(Instruction::Pop);
                    }
                }
                Ok(())
            }
            _ => Err(self.unsupported("expression", expression.span)),
        }
    }

    fn emit_binary(&mut self, operator: &str, span: Span) -> Result<(), CompileError> {
        let operator = BinaryOperator::from_spelling(operator)
            .ok_or_else(|| self.unsupported("binary operator", span))?;
        self.code.push(match operator {
            BinaryOperator::Add => Instruction::Add,
            BinaryOperator::Subtract => Instruction::Subtract,
            BinaryOperator::Multiply => Instruction::Multiply,
            BinaryOperator::Divide => Instruction::Divide,
            BinaryOperator::StrictEqual => Instruction::StrictEqual,
            operator => Instruction::Binary(operator),
        });
        Ok(())
    }

    fn compound_assignment(
        &mut self,
        operator: &str,
        target: &Expr,
        value: &Expr,
        span: Span,
    ) -> Result<(), CompileError> {
        if !matches!(
            operator,
            "+=" | "-="
                | "*="
                | "/="
                | "%="
                | "**="
                | "&="
                | "|="
                | "^="
                | "<<="
                | ">>="
                | ">>>="
                | "&&="
                | "||="
                | "??="
        ) {
            return Err(self.unsupported("assignment operator", span));
        }
        self.resolve_update_reference(target)?;
        self.code.push(Instruction::LoadReference);
        if matches!(operator, "&&=" | "||=" | "??=") {
            self.code.push(Instruction::Duplicate);
            if operator == "??=" {
                let rhs = self.emit(Instruction::JumpIfNullish(0));
                self.code.push(Instruction::DiscardReference);
                let end = self.emit(Instruction::Jump(0));
                let start = self.offset()?;
                self.patch(rhs, start);
                self.code.push(Instruction::Pop);
                self.expression(value)?;
                self.code.push(Instruction::Duplicate);
                self.code.push(Instruction::StoreReference);
                let finish = self.offset()?;
                self.patch(end, finish);
            } else {
                if operator == "||=" {
                    self.code.push(Instruction::LogicalNot);
                }
                let short = self.emit(Instruction::JumpIfFalse(0));
                self.code.push(Instruction::Pop);
                self.expression(value)?;
                self.code.push(Instruction::Duplicate);
                self.code.push(Instruction::StoreReference);
                let end = self.emit(Instruction::Jump(0));
                let start = self.offset()?;
                self.patch(short, start);
                self.code.push(Instruction::DiscardReference);
                let finish = self.offset()?;
                self.patch(end, finish);
            }
        } else {
            self.expression(value)?;
            self.emit_binary(operator.strip_suffix('=').expect("compound suffix"), span)?;
            self.code.push(Instruction::Duplicate);
            self.code.push(Instruction::StoreReference);
        }
        Ok(())
    }

    fn resolve_update_reference(&mut self, target: &Expr) -> Result<(), CompileError> {
        let target = unparenthesized(target);
        match &target.kind {
            ExprKind::Identifier(name) => {
                let name = self.pool(Constant::String(name.as_str().into()))?;
                self.code.push(Instruction::ResolveName {
                    name,
                    strict: self.strict,
                });
            }
            ExprKind::Member {
                object,
                property,
                computed,
                optional: false,
            } => {
                self.expression(object)?;
                self.property_key(property, *computed)?;
                self.code.push(Instruction::ResolveProperty {
                    strict: self.strict,
                });
            }
            _ => return Err(self.unsupported("update target", target.span)),
        }
        Ok(())
    }

    fn function_template(
        &self,
        function: &Function,
        arrow: bool,
    ) -> Result<FunctionTemplate, CompileError> {
        if function.asynchronous || function.generator {
            return Err(self.unsupported("async/generator function", function.span));
        }
        let parameters = function
            .parameters
            .iter()
            .map(|parameter| {
                if let ExprKind::Identifier(name) = &parameter.kind {
                    Ok(JsString::from(name.as_str()))
                } else {
                    Err(self.unsupported("non-simple function parameter", parameter.span))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let strict = self.strict || function.strict;
        let mut compiler = Compiler::empty(strict);
        match &function.body {
            FunctionBody::Block(body) => {
                let StmtKind::Block(statements) = &body.kind else {
                    return Err(self.unsupported("function block", body.span));
                };
                let lexical = scope_bindings(statements, false, true, false)?;
                if lexical
                    .iter()
                    .any(|binding| parameters.contains(&binding.name))
                {
                    return Err(
                        self.unsupported("parameter/lexical binding conflict", function.span)
                    );
                }
                let mut bindings = scope_bindings(statements, true, false, true)?;
                for name in &parameters {
                    if !bindings.iter().any(|binding| &binding.name == name) {
                        bindings.push(ScopeBinding {
                            name: name.clone(),
                            kind: BindingKind::Var,
                        });
                    }
                }
                compiler.enter_scope(ScopeKind::Function, bindings)?;
                let terminal = if strict {
                    // Simple parameters permit the strict body lexical bindings
                    // in the Function record itself (FDI step 31).
                    let lexical = scope_bindings(statements, false, true, false)?;
                    let scope = compiler.scopes.last().expect("function scope").entry;
                    let Instruction::EnterScope(index) = compiler.code[scope as usize] else {
                        unreachable!()
                    };
                    let Constant::Scope(plan) = &mut compiler.constants[index as usize] else {
                        unreachable!()
                    };
                    for binding in lexical {
                        let slot = u32::try_from(plan.bindings.len())
                            .map_err(|_| CompileError::new("binding slot overflow", None))?;
                        compiler
                            .scopes
                            .last_mut()
                            .expect("function scope")
                            .bindings
                            .insert(binding.name.to_string_lossy(), (slot, binding.kind));
                        plan.bindings.push(binding);
                    }
                    compiler.attach_functions(statements, scope)?;
                    compiler.statements(statements)?
                } else {
                    let target = compiler.scopes.last().expect("function scope").entry;
                    compiler.block_with_functions(statements, Some(target))?
                };
                if !terminal {
                    compiler.constant(Constant::Undefined)?;
                    compiler.code.push(Instruction::Return);
                }
            }
            FunctionBody::Expression(expression) => {
                let mut bindings = vec![];
                for name in &parameters {
                    if !bindings
                        .iter()
                        .any(|binding: &ScopeBinding| &binding.name == name)
                    {
                        bindings.push(ScopeBinding {
                            name: name.clone(),
                            kind: BindingKind::Var,
                        });
                    }
                }
                compiler.enter_scope(ScopeKind::Function, bindings)?;
                compiler.expression(expression)?;
                compiler.code.push(Instruction::Return);
            }
        }
        let name = match function.name.as_ref().map(|name| &name.kind) {
            Some(ExprKind::Identifier(name)) => Some(JsString::from(name.as_str())),
            None => None,
            _ => return Err(self.unsupported("function name", function.span)),
        };
        Ok(FunctionTemplate {
            self_binding: name.is_some() && !arrow,
            arguments_object: !arrow
                && !parameters
                    .iter()
                    .any(|name| name.code_units() == JsString::from("arguments").code_units())
                && !body_shadows_arguments(function),
            name,
            parameters,
            arrow,
            strict,
            body: compiler.finish()?,
        })
    }

    fn construct(&mut self, callee: &Expr, arguments: &[Expr]) -> Result<(), CompileError> {
        self.expression(callee)?;
        self.code.push(Instruction::BeginConstruct);
        self.arguments(arguments)?;
        self.code.push(Instruction::Invoke);
        Ok(())
    }

    fn arguments(&mut self, arguments: &[Expr]) -> Result<(), CompileError> {
        for argument in arguments {
            if let ExprKind::Spread(value) = &argument.kind {
                self.expression(value)?;
                self.code.push(Instruction::SpreadArguments);
            } else {
                self.expression(argument)?;
                self.code.push(Instruction::AppendArgument);
            }
        }
        Ok(())
    }

    fn property_key(&mut self, property: &Expr, computed: bool) -> Result<(), CompileError> {
        if computed {
            return self.expression(property);
        }
        match &property.kind {
            ExprKind::Identifier(name) => self.constant(Constant::String(name.as_str().into())),
            ExprKind::Literal(Literal::String { value, .. }) => {
                self.constant(Constant::String(JsString::from_code_units_slice(value)))
            }
            _ => Err(self.unsupported("property name", property.span)),
        }
    }

    fn member_value(&mut self, member: &Expr, delete: bool) -> Result<(), CompileError> {
        self.chain_value(
            member,
            if delete {
                ChainUse::Delete
            } else {
                ChainUse::Value
            },
        )
    }

    fn chain_value(&mut self, expression: &Expr, usage: ChainUse) -> Result<(), CompileError> {
        let mut shortcuts = vec![];
        self.chain_inner(expression, usage, &mut shortcuts)?;
        if shortcuts.is_empty() {
            return Ok(());
        }
        let mut exits = vec![self.emit(Instruction::Jump(0))];
        for (branch, cleanup) in shortcuts {
            let short = self.offset()?;
            self.patch(branch, short);
            for _ in 0..cleanup {
                self.code.push(Instruction::Pop);
            }
            if matches!(usage, ChainUse::Callee) {
                self.constant(Constant::Undefined)?;
            }
            self.constant(if matches!(usage, ChainUse::Delete) {
                Constant::Boolean(true)
            } else {
                Constant::Undefined
            })?;
            exits.push(self.emit(Instruction::Jump(0)));
        }
        let finished = self.offset()?;
        for exit in exits {
            self.patch(exit, finished);
        }
        Ok(())
    }

    fn chain_inner(
        &mut self,
        expression: &Expr,
        usage: ChainUse,
        shortcuts: &mut Vec<(usize, usize)>,
    ) -> Result<(), CompileError> {
        match &expression.kind {
            ExprKind::Member {
                object,
                property,
                computed,
                optional,
            } => {
                if matches!(object.kind, ExprKind::Member { .. } | ExprKind::Call { .. }) {
                    self.chain_inner(object, ChainUse::Value, shortcuts)?;
                } else {
                    self.expression(object)?;
                }
                if *optional {
                    self.code.push(Instruction::Duplicate);
                    shortcuts.push((self.emit(Instruction::JumpIfNullish(0)), 1));
                }
                self.property_key(property, *computed)?;
                if matches!(usage, ChainUse::Value) {
                    self.code.push(Instruction::GetProperty {
                        strict: self.strict,
                    });
                } else {
                    self.code.push(Instruction::ResolveProperty {
                        strict: self.strict,
                    });
                    self.code.push(if matches!(usage, ChainUse::Delete) {
                        Instruction::DeleteReference
                    } else {
                        Instruction::LoadReferenceCallTarget
                    });
                }
            }
            ExprKind::Call {
                callee,
                arguments,
                optional,
            } => {
                self.chain_inner(callee, ChainUse::Callee, shortcuts)?;
                if *optional {
                    // Receiver lies below callee. Both roots are discarded only
                    // on this branch; no pending invocation has been opened yet.
                    self.code.push(Instruction::Duplicate);
                    shortcuts.push((self.emit(Instruction::JumpIfNullish(0)), 2));
                }
                self.code.push(Instruction::BeginMethodCall);
                self.arguments(arguments)?;
                self.code.push(Instruction::Invoke);
                match usage {
                    ChainUse::Callee => {
                        self.code.push(Instruction::AsCallTarget);
                    }
                    ChainUse::Delete => {
                        self.code.push(Instruction::Pop);
                        self.constant(Constant::Boolean(true))?;
                    }
                    ChainUse::Value => {}
                }
            }
            ExprKind::Parenthesized(inner) if matches!(usage, ChainUse::Callee) => {
                // Grouping ends short-circuit propagation, but preserves a
                // non-short-circuited member Reference and its method receiver.
                self.chain_value(inner, ChainUse::Callee)?;
            }
            ExprKind::Identifier(name) if matches!(usage, ChainUse::Callee) => {
                let name = self.pool(Constant::String(name.as_str().into()))?;
                self.code.push(Instruction::ResolveName {
                    name,
                    strict: self.strict,
                });
                self.code.push(Instruction::LoadReferenceCallTarget);
            }
            _ => {
                self.expression(expression)?;
                if matches!(usage, ChainUse::Callee) {
                    self.code.push(Instruction::AsCallTarget);
                }
            }
        }
        Ok(())
    }

    fn literal(&mut self, literal: &Literal, span: Span) -> Result<(), CompileError> {
        let constant = match literal {
            Literal::Null => Constant::Null,
            Literal::Boolean(value) => Constant::Boolean(*value),
            Literal::String { value, .. } => {
                Constant::String(JsString::from_code_units(value.clone()))
            }
            Literal::Number { raw, kind } => match parse_numeric_literal(raw, *kind)
                .map_err(|error| CompileError::new(error.to_string(), Some(span)))?
            {
                Value::Number(value) => Constant::Number(value),
                Value::BigInt(value) => Constant::BigInt(value),
                _ => return Err(self.unsupported("numeric literal", span)),
            },
            Literal::RegExp(_) => return Err(self.unsupported("RegExp literal", span)),
        };
        self.constant(constant)
    }

    fn pool(&mut self, constant: Constant) -> Result<u32, CompileError> {
        let index = u32::try_from(self.constants.len())
            .map_err(|_| CompileError::new("constant pool overflow", None))?;
        self.constants.push(constant);
        Ok(index)
    }

    fn constant(&mut self, constant: Constant) -> Result<(), CompileError> {
        let index = self.pool(constant)?;
        self.code.push(Instruction::LoadConstant(index));
        Ok(())
    }

    fn binding(&self, name: &str) -> Option<(u32, u32, BindingKind)> {
        self.scopes.iter().rev().find_map(|scope| {
            scope
                .bindings
                .get(name)
                .map(|(slot, kind)| (scope.entry, *slot, *kind))
        })
    }

    fn load_name(&mut self, name: &str) -> Result<(), CompileError> {
        let instruction = if let Some((scope, slot, _)) = self.local_binding(name) {
            Instruction::LoadBinding { scope, slot }
        } else {
            Instruction::GetName {
                name: self.pool(Constant::String(name.into()))?,
                strict: self.strict,
            }
        };
        self.code.push(instruction);
        Ok(())
    }

    fn offset(&self) -> Result<u32, CompileError> {
        u32::try_from(self.code.len())
            .map_err(|_| CompileError::new("instruction stream overflow", None))
    }

    fn local_binding(&self, name: &str) -> Option<(u32, u32, BindingKind)> {
        self.binding(name).filter(|(entry, _, _)| {
            self.scopes
                .iter()
                .any(|scope| scope.entry == *entry && scope.kind != ScopeKind::Global)
        })
    }

    fn emit(&mut self, instruction: Instruction) -> usize {
        let offset = self.code.len();
        self.code.push(instruction);
        offset
    }
    fn patch(&mut self, offset: usize, target: u32) {
        self.code[offset] = match self.code[offset] {
            Instruction::Jump(_) => Instruction::Jump(target),
            Instruction::JumpIfFalse(_) => Instruction::JumpIfFalse(target),
            Instruction::JumpIfNullish(_) => Instruction::JumpIfNullish(target),
            _ => unreachable!("only branches are patched"),
        };
    }
    fn unsupported(&self, syntax: &str, span: Span) -> CompileError {
        CompileError::new(format!("unsupported bytecode {syntax}"), Some(span))
    }
}

// Validate the supported statement tree even after terminal statements: var
// declarations are instantiated before any reachable code executes.
fn collect_vars(statements: &[Stmt], vars: &mut Vec<String>) -> Result<(), CompileError> {
    for statement in statements {
        match &statement.kind {
            StmtKind::Variable { kind, declarations } => {
                for declaration in declarations {
                    let ExprKind::Identifier(name) = &declaration.binding.kind else {
                        return Err(CompileError::new(
                            "unsupported binding pattern",
                            Some(declaration.binding.span),
                        ));
                    };
                    if *kind == VariableKind::Var && !vars.contains(name) {
                        vars.push(name.clone());
                    }
                }
            }
            StmtKind::Block(body) => collect_vars(body, vars)?,
            StmtKind::If {
                consequent,
                alternate,
                ..
            } => {
                collect_vars(std::slice::from_ref(consequent), vars)?;
                if let Some(alternate) = alternate {
                    collect_vars(std::slice::from_ref(alternate), vars)?;
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Labeled { body, .. } => collect_vars(std::slice::from_ref(body), vars)?,
            StmtKind::For { init, body, .. } => {
                if let Some(ForInit::Variable { kind, declarations }) = init {
                    for declaration in declarations {
                        let ExprKind::Identifier(name) = &declaration.binding.kind else {
                            return Err(CompileError::new(
                                "unsupported binding pattern",
                                Some(declaration.binding.span),
                            ));
                        };
                        if *kind == VariableKind::Var && !vars.contains(name) {
                            vars.push(name.clone());
                        }
                    }
                }
                collect_vars(std::slice::from_ref(body), vars)?;
            }
            StmtKind::FunctionDeclaration(_) => {}
            StmtKind::Empty
            | StmtKind::Debugger
            | StmtKind::Expression(_)
            | StmtKind::Return(_)
            | StmtKind::Throw(_) => {}
            StmtKind::Break(_) | StmtKind::Continue(_) => {}
            _ => {
                return Err(CompileError::new(
                    "unsupported bytecode statement",
                    Some(statement.span),
                ));
            }
        }
    }
    Ok(())
}

fn scope_bindings(
    statements: &[Stmt],
    include_vars: bool,
    include_lexical: bool,
    include_functions: bool,
) -> Result<Vec<ScopeBinding>, CompileError> {
    let mut vars = vec![];
    collect_vars(statements, &mut vars)?;
    let mut lexical = vec![];
    let mut function_names = vec![];
    for statement in statements {
        if let StmtKind::FunctionDeclaration(function) = &statement.kind {
            let Some(ExprKind::Identifier(name)) = function.name.as_ref().map(|name| &name.kind)
            else {
                return Err(CompileError::new(
                    "anonymous function declaration",
                    Some(function.span),
                ));
            };
            if include_vars {
                if !vars.contains(name) {
                    vars.push(name.clone());
                }
            } else if include_functions && include_lexical {
                if vars.contains(name) || function_names.contains(name) {
                    return Err(CompileError::new(
                        "duplicate lexical function binding",
                        Some(function.span),
                    ));
                }
                function_names.push(name.clone());
                lexical.push((name.clone(), BindingKind::Let));
            }
        }
    }
    for statement in statements {
        if let StmtKind::Variable { kind, declarations } = &statement.kind {
            if *kind == VariableKind::Var {
                continue;
            }
            for declaration in declarations {
                let ExprKind::Identifier(name) = &declaration.binding.kind else {
                    unreachable!("collect_vars checked patterns")
                };
                if vars.contains(name) || lexical.iter().any(|(previous, _)| previous == name) {
                    return Err(CompileError::new(
                        "duplicate lexical binding",
                        Some(declaration.span),
                    ));
                }
                lexical.push((
                    name.clone(),
                    if *kind == VariableKind::Let {
                        BindingKind::Let
                    } else {
                        BindingKind::Const
                    },
                ));
            }
        }
    }
    let mut bindings = vec![];
    if include_vars {
        bindings.extend(vars.into_iter().map(|name| ScopeBinding {
            name: name.as_str().into(),
            kind: BindingKind::Var,
        }));
    }
    if include_lexical {
        bindings.extend(lexical.into_iter().map(|(name, kind)| ScopeBinding {
            name: name.as_str().into(),
            kind,
        }));
    }
    Ok(bindings)
}

fn unparenthesized(mut expression: &Expr) -> &Expr {
    while let ExprKind::Parenthesized(inner) = &expression.kind {
        expression = inner;
    }
    expression
}

#[derive(Clone, Copy)]
enum ChainUse {
    Value,
    Callee,
    Delete,
}

fn body_shadows_arguments(function: &Function) -> bool {
    let FunctionBody::Block(body) = &function.body else {
        return false;
    };
    let StmtKind::Block(statements) = &body.kind else {
        return false;
    };
    statements.iter().any(|statement| match &statement.kind {
        StmtKind::FunctionDeclaration(function) => matches!(function.name.as_ref().map(|name| &name.kind), Some(ExprKind::Identifier(name)) if name == "arguments"),
        StmtKind::Variable { kind, declarations } if *kind != VariableKind::Var => declarations.iter().any(|declaration| matches!(&declaration.binding.kind, ExprKind::Identifier(name) if name == "arguments")),
        _ => false,
    })
}
