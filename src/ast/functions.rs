use super::*;

/// Owned function syntax, shared by functions, arrows, and methods.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// Entire function or method range.
    pub span: Span,
    /// Optional decoded function name and its source range.
    pub name: Option<Expr>,
    /// Binding patterns, defaults (`Assignment`), and rest (`Spread`) parameters.
    pub parameters: Vec<Expr>,
    /// Statement body or arrow expression body.
    pub body: FunctionBody,
    /// Whether this is an async function.
    pub asynchronous: bool,
    /// Whether this is a generator.
    pub generator: bool,
    /// Whether the function's code is strict.
    pub strict: bool,
}

/// A function body, including the concise expression form of arrows.
#[derive(Clone, Debug, PartialEq)]
pub enum FunctionBody {
    /// A block, including its braces and source range.
    Block(Box<Stmt>),
    /// A concise arrow body.
    Expression(Box<Expr>),
}

/// The syntactic kind of an object or class method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MethodKind {
    /// A normal, async, or generator method.
    Method,
    /// A getter with no parameters.
    Get,
    /// A setter with exactly one non-rest parameter.
    Set,
    /// A non-static class constructor.
    Constructor,
}

/// An owned class definition.
#[derive(Clone, Debug, PartialEq)]
pub struct Class {
    /// Entire class range.
    pub span: Span,
    /// Optional class name.
    pub name: Option<Expr>,
    /// Optional superclass expression.
    pub super_class: Option<Expr>,
    /// Elements in source order, excluding empty semicolons.
    pub elements: Vec<ClassElement>,
}

/// A class method, field, or static initialization block.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassElement {
    /// Entire element range including modifiers and terminators.
    pub span: Span,
    /// Property name, absent for static blocks.
    pub key: Option<Expr>,
    /// Whether the property name is computed.
    pub computed: bool,
    /// Whether the element is static.
    pub is_static: bool,
    /// Element structure.
    pub kind: ClassElementKind,
}

/// Structure of a class element.
#[derive(Clone, Debug, PartialEq)]
pub enum ClassElementKind {
    /// A method, accessor, or constructor.
    Method {
        /// Method code.
        function: Box<Function>,
        /// Method category.
        kind: MethodKind,
    },
    /// A field with an optional initializer.
    Field(Option<Expr>),
    /// A static block including braces.
    StaticBlock(Box<Stmt>),
}

impl Function {
    /// Visits all expression syntax, including parameters and nested functions.
    pub(crate) fn visit_expressions(&self, visitor: &mut impl FnMut(&Expr)) {
        for expression in self.expressions() {
            expression.visit(visitor);
        }
    }

    pub(super) fn expressions(&self) -> Vec<&Expr> {
        let mut expressions: Vec<_> = self.name.iter().chain(self.parameters.iter()).collect();
        match &self.body {
            FunctionBody::Expression(expr) => expressions.push(expr),
            FunctionBody::Block(block) => expressions.extend(block.expression_roots()),
        }
        expressions
    }

    pub(super) fn validate(&self, source: &SourceText<'_>, parent: Span) -> Result<(), AstError> {
        validate_span(source, parent, self.span)?;
        for expr in self.name.iter().chain(self.parameters.iter()) {
            validate_expr(source, self.span, expr)?;
        }
        match &self.body {
            FunctionBody::Expression(expr) => validate_expr(source, self.span, expr),
            FunctionBody::Block(block) => validate_stmt(source, self.span, block),
        }
    }
}

impl Class {
    pub(super) fn expressions(&self) -> Vec<&Expr> {
        let mut expressions: Vec<_> = self.name.iter().chain(self.super_class.iter()).collect();
        for element in &self.elements {
            expressions.extend(element.key.iter());
            match &element.kind {
                ClassElementKind::Field(value) => expressions.extend(value.iter()),
                ClassElementKind::Method { function, .. } => {
                    expressions.extend(function.expressions())
                }
                ClassElementKind::StaticBlock(block) => {
                    expressions.extend(block.expression_roots())
                }
            }
        }
        expressions
    }

    pub(super) fn validate(&self, source: &SourceText<'_>, parent: Span) -> Result<(), AstError> {
        validate_span(source, parent, self.span)?;
        for expr in self.name.iter().chain(self.super_class.iter()) {
            validate_expr(source, self.span, expr)?;
        }
        for element in &self.elements {
            validate_span(source, self.span, element.span)?;
            if let Some(key) = &element.key {
                validate_expr(source, element.span, key)?;
            }
            match &element.kind {
                ClassElementKind::Field(value) => {
                    if let Some(value) = value {
                        validate_expr(source, element.span, value)?;
                    }
                }
                ClassElementKind::Method { function, .. } => {
                    function.validate(source, element.span)?
                }
                ClassElementKind::StaticBlock(block) => validate_stmt(source, element.span, block)?,
            }
        }
        Ok(())
    }
}

impl Stmt {
    pub(crate) fn expression_roots(&self) -> Vec<&Expr> {
        let mut expressions = vec![];
        match &self.kind {
            StmtKind::ImportDeclaration(import) => {
                expressions.extend(import.specifiers.iter().map(|s| &s.local))
            }
            StmtKind::ExportDeclaration(export) => match &export.kind {
                ExportKind::Declaration(stmt) | ExportKind::DefaultDeclaration(stmt) => {
                    expressions.extend(stmt.expression_roots())
                }
                ExportKind::DefaultExpression(expr) => expressions.push(expr),
                _ => {}
            },
            StmtKind::Empty | StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Debugger => {}
            StmtKind::Expression(expr) | StmtKind::Throw(expr) => expressions.push(expr),
            StmtKind::Return(value) => expressions.extend(value.iter()),
            StmtKind::Block(body) => {
                for stmt in body {
                    expressions.extend(stmt.expression_roots());
                }
            }
            StmtKind::If {
                test,
                consequent,
                alternate,
            } => {
                expressions.push(test);
                expressions.extend(consequent.expression_roots());
                if let Some(alternate) = alternate {
                    expressions.extend(alternate.expression_roots());
                }
            }
            StmtKind::While { test, body } | StmtKind::DoWhile { test, body } => {
                expressions.push(test);
                expressions.extend(body.expression_roots());
            }
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => {
                if let Some(init) = init {
                    expressions.extend(init.expressions());
                }
                expressions.extend(test.iter());
                expressions.extend(update.iter());
                expressions.extend(body.expression_roots());
            }
            StmtKind::ForEach {
                left, right, body, ..
            } => {
                expressions.extend(left.expressions());
                expressions.push(right);
                expressions.extend(body.expression_roots());
            }
            StmtKind::Labeled { body, .. } => expressions.extend(body.expression_roots()),
            StmtKind::Switch {
                discriminant,
                cases,
            } => {
                expressions.push(discriminant);
                for case in cases {
                    expressions.extend(case.test.iter());
                    for stmt in &case.consequent {
                        expressions.extend(stmt.expression_roots());
                    }
                }
            }
            StmtKind::Try {
                block,
                handler,
                finalizer,
            } => {
                expressions.extend(block.expression_roots());
                if let Some(handler) = handler {
                    expressions.extend(handler.param.iter());
                    expressions.extend(handler.body.expression_roots());
                }
                if let Some(finalizer) = finalizer {
                    expressions.extend(finalizer.expression_roots());
                }
            }
            StmtKind::With { object, body } => {
                expressions.push(object);
                expressions.extend(body.expression_roots());
            }
            StmtKind::Variable { declarations, .. } => {
                expressions.extend(declaration_expressions(declarations))
            }
            StmtKind::FunctionDeclaration(function) => expressions.extend(function.expressions()),
            StmtKind::ClassDeclaration(class) => expressions.extend(class.expressions()),
        }
        expressions
    }
}

fn declaration_expressions(declarations: &[VariableDeclarator]) -> Vec<&Expr> {
    declarations
        .iter()
        .flat_map(|d| std::iter::once(&d.binding).chain(d.init.iter()))
        .collect()
}

impl ForInit {
    fn expressions(&self) -> Vec<&Expr> {
        match self {
            Self::Expression(expr) => vec![expr],
            Self::Variable { declarations, .. } => declaration_expressions(declarations),
        }
    }
}
