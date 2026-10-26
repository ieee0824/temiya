use std::{collections::HashMap, fmt};

use crate::{
    ast::{Expr, ExprKind, Literal, Program, Stmt, StmtKind},
    lexer::Span,
    parser::parse_script,
    runtime::{JsString, Value, parse_numeric_literal},
};

use super::{Bytecode, Constant, Instruction, VerifiedBytecode, VerifyError};

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

    /// Lowers an owned AST and verifies the resulting bytecode.
    pub fn compile_program(program: &Program) -> Result<VerifiedBytecode, CompileError> {
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

struct Compiler {
    constants: Vec<Constant>,
    code: Vec<Instruction>,
    locals: HashMap<String, u32>,
    completion_local: u32,
}

impl Compiler {
    fn new(program: &Program) -> Result<Self, CompileError> {
        let mut locals = HashMap::new();
        collect_bindings(&program.body, &mut locals)?;
        let completion_local = u32::try_from(locals.len())
            .map_err(|_| CompileError::new("too many local bindings", None))?;
        Ok(Self {
            constants: vec![],
            code: vec![],
            locals,
            completion_local,
        })
    }

    fn compile(mut self, program: &Program) -> Result<VerifiedBytecode, CompileError> {
        let terminated = self.statements(&program.body)?;
        if !terminated {
            self.code
                .push(Instruction::LoadLocal(self.completion_local));
            self.code.push(Instruction::Complete);
        }
        Bytecode {
            constants: self.constants,
            instructions: self.code,
            local_count: self.completion_local + 1,
        }
        .verify()
        .map_err(|error: VerifyError| CompileError::new(error.to_string(), None))
    }

    fn statements(&mut self, statements: &[Stmt]) -> Result<bool, CompileError> {
        for statement in statements {
            if self.statement(statement)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn statement(&mut self, statement: &Stmt) -> Result<bool, CompileError> {
        match &statement.kind {
            StmtKind::Empty | StmtKind::Debugger => Ok(false),
            StmtKind::Expression(expression) => {
                self.expression(expression)?;
                self.code
                    .push(Instruction::StoreLocal(self.completion_local));
                Ok(false)
            }
            StmtKind::Variable { declarations, .. } => {
                for declaration in declarations {
                    let ExprKind::Identifier(name) = &declaration.binding.kind else {
                        return Err(self.unsupported("binding pattern", declaration.binding.span));
                    };
                    if let Some(init) = &declaration.init {
                        self.expression(init)?;
                    } else {
                        self.constant(Constant::Undefined)?;
                    }
                    self.code.push(Instruction::StoreLocal(
                        self.local(name, declaration.binding.span)?,
                    ));
                }
                Ok(false)
            }
            StmtKind::Block(body) => self.statements(body),
            StmtKind::If {
                test,
                consequent,
                alternate,
            } => {
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
            StmtKind::While { test, body } => {
                let start = self.offset()?;
                self.expression(test)?;
                let exit = self.emit(Instruction::JumpIfFalse(0));
                if self.statement(body)? {
                    return Err(self.unsupported("terminal while body", statement.span));
                }
                self.code.push(Instruction::Jump(start));
                let end = self.offset()?;
                self.patch(exit, end);
                Ok(false)
            }
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
            ExprKind::Identifier(name) => {
                self.code
                    .push(Instruction::LoadLocal(self.local(name, expression.span)?));
                Ok(())
            }
            ExprKind::Parenthesized(inner) => self.expression(inner),
            ExprKind::Assignment {
                operator,
                target,
                value,
            } if operator == "=" => {
                let ExprKind::Identifier(name) = &target.kind else {
                    return Err(self.unsupported("assignment target", target.span));
                };
                self.expression(value)?;
                self.code.push(Instruction::Duplicate);
                self.code
                    .push(Instruction::StoreLocal(self.local(name, target.span)?));
                Ok(())
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } if matches!(operator.as_str(), "+" | "-" | "*" | "/" | "===") => {
                self.expression(left)?;
                self.expression(right)?;
                self.code.push(match operator.as_str() {
                    "+" => Instruction::Add,
                    "-" => Instruction::Subtract,
                    "*" => Instruction::Multiply,
                    "/" => Instruction::Divide,
                    _ => Instruction::StrictEqual,
                });
                Ok(())
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
                _ => return Err(self.unsupported("BigInt literal", span)),
            },
            Literal::RegExp(_) => return Err(self.unsupported("RegExp literal", span)),
        };
        self.constant(constant)
    }

    fn constant(&mut self, constant: Constant) -> Result<(), CompileError> {
        let index = u32::try_from(self.constants.len())
            .map_err(|_| CompileError::new("constant pool overflow", None))?;
        self.constants.push(constant);
        self.code.push(Instruction::LoadConstant(index));
        Ok(())
    }

    fn local(&self, name: &str, span: Span) -> Result<u32, CompileError> {
        self.locals
            .get(name)
            .copied()
            .ok_or_else(|| CompileError::new(format!("unbound local {name}"), Some(span)))
    }

    fn offset(&self) -> Result<u32, CompileError> {
        u32::try_from(self.code.len())
            .map_err(|_| CompileError::new("instruction stream overflow", None))
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
            _ => unreachable!("only branches are patched"),
        };
    }
    fn unsupported(&self, syntax: &str, span: Span) -> CompileError {
        CompileError::new(format!("unsupported bytecode {syntax}"), Some(span))
    }
}

fn collect_bindings(
    statements: &[Stmt],
    locals: &mut HashMap<String, u32>,
) -> Result<(), CompileError> {
    for statement in statements {
        match &statement.kind {
            StmtKind::Variable { declarations, .. } => {
                for declaration in declarations {
                    let ExprKind::Identifier(name) = &declaration.binding.kind else {
                        return Err(CompileError::new(
                            "unsupported binding pattern",
                            Some(declaration.binding.span),
                        ));
                    };
                    if !locals.contains_key(name) {
                        let index = u32::try_from(locals.len())
                            .map_err(|_| CompileError::new("too many local bindings", None))?;
                        locals.insert(name.clone(), index);
                    }
                }
            }
            StmtKind::Block(body) => collect_bindings(body, locals)?,
            StmtKind::If {
                consequent,
                alternate,
                ..
            } => {
                collect_bindings(std::slice::from_ref(consequent), locals)?;
                if let Some(alternate) = alternate {
                    collect_bindings(std::slice::from_ref(alternate), locals)?;
                }
            }
            StmtKind::While { body, .. } => collect_bindings(std::slice::from_ref(body), locals)?,
            _ => {}
        }
    }
    Ok(())
}
