//! Owned syntax nodes shared by parsing, static semantics, and evaluation.

use crate::{lexer::Span, source::SourceText};

mod functions;
mod modules;
pub use functions::{Class, ClassElement, ClassElementKind, Function, FunctionBody, MethodKind};
pub use modules::{
    ExportDeclaration, ExportKind, ExportSpecifier, ImportAttribute, ImportDeclaration, ImportName,
    ImportSpecifier, ModuleRequest,
};

/// An owned expression with a half-open UTF-8 byte span in its original source.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    /// Source range, including delimiters and intervening trivia.
    pub span: Span,
    /// Expression structure. All strings and child nodes are owned.
    pub kind: ExprKind,
}

/// Expression structure. New syntax is added as explicit variants.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ExprKind {
    /// A decoded identifier name (without Unicode normalization).
    Identifier(String),
    /// A dynamic import call; evaluated at runtime, not a static request.
    ImportCall {
        /// Module specifier expression.
        specifier: Box<Expr>,
        /// Optional import options expression.
        options: Option<Box<Expr>>,
    },
    /// A decoded private identifier, excluding the leading `#`.
    PrivateIdentifier(String),
    /// A function expression.
    Function(Box<Function>),
    /// An arrow function, with lexical this/super semantics.
    Arrow(Box<Function>),
    /// A class expression.
    Class(Box<Class>),
    /// An object method or accessor, used as a property value syntax node.
    Method {
        /// Method code.
        function: Box<Function>,
        /// Method category.
        kind: MethodKind,
    },
    /// A super reference, valid only as a property/call base.
    Super,
    /// A grammar-defined meta-property such as `new.target`.
    MetaProperty {
        /// Meta-property prefix.
        meta: String,
        /// Property name.
        property: String,
    },
    /// A generator yield expression.
    Yield {
        /// Optional yielded expression.
        argument: Option<Box<Expr>>,
        /// Whether this is `yield*`.
        delegate: bool,
    },
    /// A literal with an owned value or raw numeric/regexp spelling.
    Literal(Literal),
    /// The `this` primary expression.
    This,
    /// Parentheses are retained for grammar restrictions and source fidelity.
    Parenthesized(Box<Expr>),
    /// A prefix unary operator.
    Unary {
        /// Original operator spelling.
        operator: String,
        /// Operand expression.
        argument: Box<Expr>,
    },
    /// A prefix or postfix increment/decrement operation.
    Update {
        /// `++` or `--`.
        operator: String,
        /// Target expression.
        argument: Box<Expr>,
        /// True for prefix, false for postfix.
        prefix: bool,
    },
    /// A binary or short-circuit logical expression.
    Binary {
        /// Original operator spelling.
        operator: String,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// A ternary conditional expression.
    Conditional {
        /// Condition expression.
        test: Box<Expr>,
        /// Expression selected when true.
        consequent: Box<Expr>,
        /// Expression selected when false.
        alternate: Box<Expr>,
    },
    /// An assignment expression; target legality is checked by the parser.
    Assignment {
        /// Original assignment operator spelling.
        operator: String,
        /// Target reference or binding pattern.
        target: Box<Expr>,
        /// Value expression.
        value: Box<Expr>,
    },
    /// A comma expression in evaluation order.
    Sequence(Vec<Expr>),
    /// An array literal; absent elements represent holes.
    Array {
        /// Elements in source order; absent elements are holes.
        elements: Vec<Option<Expr>>,
        /// Whether a comma follows the final non-hole element.
        trailing_comma: bool,
    },
    /// An object literal with source-ordered properties.
    Object(Vec<ObjectProperty>),
    /// A property access, including computed and optional access.
    Member {
        /// Base expression.
        object: Box<Expr>,
        /// Property name or computed expression.
        property: Box<Expr>,
        /// True when the property is evaluated inside brackets.
        computed: bool,
        /// True when introduced with `?.`.
        optional: bool,
    },
    /// A function call.
    Call {
        /// Called expression.
        callee: Box<Expr>,
        /// Arguments in source order; spread uses `Spread` nodes.
        arguments: Vec<Expr>,
        /// True for optional calls.
        optional: bool,
    },
    /// A constructor invocation.
    New {
        /// Constructor expression.
        callee: Box<Expr>,
        /// Arguments in source order.
        arguments: Vec<Expr>,
    },
    /// A spread element, valid only in a surrounding permitted context.
    Spread(Box<Expr>),
    /// A tagged or untagged template.
    Template {
        /// Optional tag expression.
        tag: Option<Box<Expr>>,
        /// Text components; exactly one more than the substitutions.
        quasis: Vec<TemplateElement>,
        /// Substitution expressions in source order.
        expressions: Vec<Expr>,
    },
}

/// An object property, retaining the span of the entire property.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectProperty {
    /// Half-open source span for this property.
    pub span: Span,
    /// Property key or spread expression.
    pub key: Expr,
    /// Property value; absent for shorthand and spread properties.
    pub value: Option<Expr>,
    /// Whether the key is computed.
    pub computed: bool,
    /// Whether this is a spread property.
    pub spread: bool,
    /// Whether this is a shorthand name, including a cover-initialized name.
    pub shorthand: bool,
    /// Whether a comma immediately follows this property in source.
    pub comma_after: bool,
}

/// An owned template text component.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateElement {
    /// Source range including component delimiters.
    pub span: Span,
    /// Raw UTF-16 value with CR/CRLF normalization.
    pub raw: Vec<u16>,
    /// Cooked value, absent when escapes are invalid in a tagged template.
    pub cooked: Option<Vec<u16>>,
}

/// Literal representation independent of the source text lifetime.
#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    /// The null literal.
    Null,
    /// A boolean value.
    Boolean(bool),
    /// A numeric spelling, deferring rounding and BigInt conversion.
    Number {
        /// Original literal spelling including separators and suffix.
        raw: String,
        /// Numeric syntax category, including strict-mode legacy forms.
        kind: NumberKind,
    },
    /// UTF-16 string data, including possible lone surrogates.
    String {
        /// Cooked UTF-16 code units.
        value: Vec<u16>,
        /// Whether strict code must reject a legacy escape.
        legacy_escape: bool,
    },
    /// Original regexp spelling including delimiters and flags.
    RegExp(String),
}

/// Numeric syntax categories retained for later static and runtime semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumberKind {
    /// A standard decimal or explicit-radix Number literal.
    Number,
    /// An integer literal ending in `n`.
    BigInt,
    /// A legacy leading-zero octal integer.
    LegacyOctal,
    /// A legacy leading-zero decimal literal.
    LegacyDecimal,
}

/// A structural or source-range problem in an AST.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AstError {
    /// The invalid node or child range.
    pub span: Span,
    /// Description of the violated invariant.
    pub message: &'static str,
}

impl std::fmt::Display for AstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for AstError {}

impl Expr {
    /// Constructs an owned node. Use `validate` to check manually built trees.
    #[must_use]
    pub fn new(span: Span, kind: ExprKind) -> Self {
        Self { span, kind }
    }

    /// Visits this node and all descendant expressions in source evaluation order.
    ///
    /// This is a syntax traversal, not execution; short-circuit branches are
    /// visited unconditionally. Node mutation during traversal is not supported.
    pub fn visit(&self, visitor: &mut impl FnMut(&Expr)) {
        visitor(self);
        for child in self.children() {
            child.visit(visitor);
        }
    }

    fn children(&self) -> Vec<&Expr> {
        match &self.kind {
            ExprKind::ImportCall { specifier, options } => std::iter::once(specifier.as_ref())
                .chain(options.iter().map(Box::as_ref))
                .collect(),
            ExprKind::Identifier(_)
            | ExprKind::PrivateIdentifier(_)
            | ExprKind::Literal(_)
            | ExprKind::This
            | ExprKind::Super
            | ExprKind::MetaProperty { .. } => vec![],
            ExprKind::Function(function)
            | ExprKind::Arrow(function)
            | ExprKind::Method { function, .. } => function.expressions(),
            ExprKind::Class(class) => class.expressions(),
            ExprKind::Yield { argument, .. } => argument.iter().map(Box::as_ref).collect(),
            ExprKind::Parenthesized(e)
            | ExprKind::Spread(e)
            | ExprKind::Unary { argument: e, .. }
            | ExprKind::Update { argument: e, .. } => vec![e],
            ExprKind::Binary { left, right, .. } => vec![left, right],
            ExprKind::Assignment { target, value, .. } => vec![target, value],
            ExprKind::Conditional {
                test,
                consequent,
                alternate,
            } => vec![test, consequent, alternate],
            ExprKind::Sequence(items) => items.iter().collect(),
            ExprKind::Array { elements, .. } => elements.iter().flatten().collect(),
            ExprKind::Object(properties) => properties
                .iter()
                .flat_map(|p| std::iter::once(&p.key).chain(p.value.iter()))
                .collect(),
            ExprKind::Member {
                object, property, ..
            } => vec![object, property],
            ExprKind::Call {
                callee, arguments, ..
            }
            | ExprKind::New { callee, arguments } => std::iter::once(callee.as_ref())
                .chain(arguments.iter())
                .collect(),
            ExprKind::Template {
                tag, expressions, ..
            } => tag
                .iter()
                .map(Box::as_ref)
                .chain(expressions.iter())
                .collect(),
        }
    }

    /// Checks source boundaries, parent containment, and template component count.
    ///
    /// This checks structure only, not ECMAScript grammar, name resolution, or
    /// operator semantics. The source must be the text this tree was built for.
    pub fn validate(&self, source: &SourceText<'_>) -> Result<(), AstError> {
        self.check_span(source, self.span)?;
        match &self.kind {
            ExprKind::Function(function)
            | ExprKind::Arrow(function)
            | ExprKind::Method { function, .. } => function.validate(source, self.span)?,
            ExprKind::Class(class) => class.validate(source, self.span)?,
            _ => {}
        }
        for child in self.children() {
            self.check_span(source, child.span)?;
            child.validate(source)?;
        }
        if let ExprKind::Object(properties) = &self.kind {
            for property in properties {
                self.check_span(source, property.span)?;
                for child in std::iter::once(&property.key).chain(property.value.iter()) {
                    if child.span.start < property.span.start || child.span.end > property.span.end
                    {
                        return Err(AstError {
                            span: child.span,
                            message: "property child is outside its parent",
                        });
                    }
                }
            }
        }
        if let ExprKind::Template {
            quasis,
            expressions,
            ..
        } = &self.kind
        {
            if quasis.len().checked_sub(1) != Some(expressions.len()) {
                return Err(AstError {
                    span: self.span,
                    message: "template component count mismatch",
                });
            }
            for quasi in quasis {
                self.check_span(source, quasi.span)?;
            }
        }
        Ok(())
    }

    fn check_span(&self, source: &SourceText<'_>, span: Span) -> Result<(), AstError> {
        source.range(span).map_err(|_| AstError {
            span,
            message: "invalid AST source range",
        })?;
        if span.start < self.span.start || span.end > self.span.end {
            return Err(AstError {
                span,
                message: "child is outside its parent",
            });
        }
        Ok(())
    }
}

/// A parsed source unit with owned statements.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    /// Entire source range, including leading and trailing trivia.
    pub span: Span,
    /// Statements in source order.
    pub body: Vec<Stmt>,
    /// Whether this body is strict code.
    pub strict: bool,
}

/// An owned statement with its complete source range.
#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    /// Half-open UTF-8 source range.
    pub span: Span,
    /// Statement structure.
    pub kind: StmtKind,
}

/// Statement syntax, extended with declarations as those parsers are added.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StmtKind {
    /// A top-level static import in Module source.
    ImportDeclaration(Box<ImportDeclaration>),
    /// A top-level export in Module source.
    ExportDeclaration(Box<ExportDeclaration>),
    /// A function declaration.
    FunctionDeclaration(Box<Function>),
    /// A class declaration.
    ClassDeclaration(Box<Class>),
    /// An explicit empty statement.
    Empty,
    /// An expression statement.
    Expression(Expr),
    /// A block of statements.
    Block(Vec<Stmt>),
    /// An if statement, including an optional else branch.
    If {
        /// Condition expression.
        test: Expr,
        /// True branch.
        consequent: Box<Stmt>,
        /// False branch.
        alternate: Option<Box<Stmt>>,
    },
    /// A while loop.
    While {
        /// Condition expression.
        test: Expr,
        /// Loop body.
        body: Box<Stmt>,
    },
    /// A do-while loop.
    DoWhile {
        /// Loop body.
        body: Box<Stmt>,
        /// Condition expression.
        test: Expr,
    },
    /// A three-clause for loop.
    For {
        /// Optional initializer.
        init: Option<ForInit>,
        /// Optional condition.
        test: Option<Expr>,
        /// Optional update.
        update: Option<Expr>,
        /// Loop body.
        body: Box<Stmt>,
    },
    /// A for-in or for-of loop.
    ForEach {
        /// Iteration target or declaration.
        left: ForInit,
        /// Enumerated or iterated expression.
        right: Expr,
        /// True for for-of, false for for-in.
        of: bool,
        /// True for for-await-of.
        asynchronous: bool,
        /// Loop body.
        body: Box<Stmt>,
    },
    /// A labelled statement.
    Labeled {
        /// Decoded label name.
        label: String,
        /// Labelled statement.
        body: Box<Stmt>,
    },
    /// A break with an optional decoded label name.
    Break(Option<String>),
    /// A continue with an optional decoded label name.
    Continue(Option<String>),
    /// A return with an optional value.
    Return(Option<Expr>),
    /// A throw with its value.
    Throw(Expr),
    /// A switch statement.
    Switch {
        /// Selected value.
        discriminant: Expr,
        /// Cases in source order.
        cases: Vec<SwitchCase>,
    },
    /// A try statement with catch and/or finally.
    Try {
        /// Protected block.
        block: Box<Stmt>,
        /// Optional catch clause.
        handler: Option<CatchClause>,
        /// Optional finally block.
        finalizer: Option<Box<Stmt>>,
    },
    /// A debugger statement.
    Debugger,
    /// A with statement, permitted only in non-strict code.
    With {
        /// Object expression.
        object: Expr,
        /// Nested statement.
        body: Box<Stmt>,
    },
    /// A variable declaration (basic identifier bindings initially).
    Variable {
        /// Declaration keyword category.
        kind: VariableKind,
        /// Declarators in source order.
        declarations: Vec<VariableDeclarator>,
    },
}

/// A for-loop initializer or iteration target.
#[derive(Clone, Debug, PartialEq)]
pub enum ForInit {
    /// An expression initializer or reference target.
    Expression(Expr),
    /// A var/let/const declaration.
    Variable {
        /// Declaration category.
        kind: VariableKind,
        /// Declarators in source order.
        declarations: Vec<VariableDeclarator>,
    },
}

/// Variable declaration category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VariableKind {
    /// A var declaration.
    Var,
    /// A let declaration.
    Let,
    /// A const declaration.
    Const,
}

/// A binding and optional initializer, retaining its own range.
#[derive(Clone, Debug, PartialEq)]
pub struct VariableDeclarator {
    /// Range from binding start through initializer, if present.
    pub span: Span,
    /// Binding syntax, initially an identifier expression.
    pub binding: Expr,
    /// Initial value expression.
    pub init: Option<Expr>,
}

/// A switch case or default clause.
#[derive(Clone, Debug, PartialEq)]
pub struct SwitchCase {
    /// Complete clause source range.
    pub span: Span,
    /// Case expression, absent for default.
    pub test: Option<Expr>,
    /// Statements in the clause.
    pub consequent: Vec<Stmt>,
}

/// A catch binding and block.
#[derive(Clone, Debug, PartialEq)]
pub struct CatchClause {
    /// Range including catch keyword, optional binding, and block.
    pub span: Span,
    /// Optional exception binding.
    pub param: Option<Expr>,
    /// Catch block.
    pub body: Box<Stmt>,
}

fn validate_span(source: &SourceText<'_>, parent: Span, span: Span) -> Result<(), AstError> {
    source.range(span).map_err(|_| AstError {
        span,
        message: "invalid AST source range",
    })?;
    if span.start < parent.start || span.end > parent.end {
        return Err(AstError {
            span,
            message: "child is outside its parent",
        });
    }
    Ok(())
}

fn validate_expr(source: &SourceText<'_>, parent: Span, expr: &Expr) -> Result<(), AstError> {
    validate_span(source, parent, expr.span)?;
    expr.validate(source)
}

fn validate_stmt(source: &SourceText<'_>, parent: Span, stmt: &Stmt) -> Result<(), AstError> {
    validate_span(source, parent, stmt.span)?;
    stmt.validate(source)
}

fn validate_declarations(
    source: &SourceText<'_>,
    parent: Span,
    declarations: &[VariableDeclarator],
) -> Result<(), AstError> {
    for declaration in declarations {
        validate_span(source, parent, declaration.span)?;
        validate_expr(source, declaration.span, &declaration.binding)?;
        if let Some(init) = &declaration.init {
            validate_expr(source, declaration.span, init)?;
        }
    }
    Ok(())
}

fn validate_for_init(
    source: &SourceText<'_>,
    parent: Span,
    init: &ForInit,
) -> Result<(), AstError> {
    match init {
        ForInit::Expression(expr) => validate_expr(source, parent, expr),
        ForInit::Variable { declarations, .. } => {
            validate_declarations(source, parent, declarations)
        }
    }
}

impl Program {
    /// Validates source boundaries and containment throughout statements and expressions.
    pub fn validate(&self, source: &SourceText<'_>) -> Result<(), AstError> {
        validate_span(source, self.span, self.span)?;
        for stmt in &self.body {
            validate_stmt(source, self.span, stmt)?;
        }
        Ok(())
    }
}

impl Stmt {
    /// Validates source boundaries and containment for this statement's subtree.
    /// This does not perform ECMAScript static semantics or control-flow analysis.
    pub fn validate(&self, source: &SourceText<'_>) -> Result<(), AstError> {
        validate_span(source, self.span, self.span)?;
        let expression = |expr: &Expr| validate_expr(source, self.span, expr);
        let statement = |stmt: &Stmt| validate_stmt(source, self.span, stmt);
        match &self.kind {
            StmtKind::ImportDeclaration(import) => import.validate(source, self.span)?,
            StmtKind::ExportDeclaration(export) => export.validate(source, self.span)?,
            StmtKind::FunctionDeclaration(function) => function.validate(source, self.span)?,
            StmtKind::ClassDeclaration(class) => class.validate(source, self.span)?,
            StmtKind::Empty | StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Debugger => {}
            StmtKind::Expression(expr) | StmtKind::Throw(expr) => expression(expr)?,
            StmtKind::Return(value) => {
                if let Some(expr) = value {
                    expression(expr)?;
                }
            }
            StmtKind::Block(body) => {
                for stmt in body {
                    statement(stmt)?;
                }
            }
            StmtKind::If {
                test,
                consequent,
                alternate,
            } => {
                expression(test)?;
                statement(consequent)?;
                if let Some(stmt) = alternate {
                    statement(stmt)?;
                }
            }
            StmtKind::While { test, body } | StmtKind::DoWhile { test, body } => {
                expression(test)?;
                statement(body)?;
            }
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => {
                if let Some(init) = init {
                    validate_for_init(source, self.span, init)?;
                }
                if let Some(test) = test {
                    expression(test)?;
                }
                if let Some(update) = update {
                    expression(update)?;
                }
                statement(body)?;
            }
            StmtKind::ForEach {
                left, right, body, ..
            } => {
                validate_for_init(source, self.span, left)?;
                expression(right)?;
                statement(body)?;
            }
            StmtKind::Labeled { body, .. } => statement(body)?,
            StmtKind::Switch {
                discriminant,
                cases,
            } => {
                expression(discriminant)?;
                for case in cases {
                    validate_span(source, self.span, case.span)?;
                    if let Some(test) = &case.test {
                        validate_expr(source, case.span, test)?;
                    }
                    for stmt in &case.consequent {
                        validate_stmt(source, case.span, stmt)?;
                    }
                }
            }
            StmtKind::Try {
                block,
                handler,
                finalizer,
            } => {
                statement(block)?;
                if let Some(handler) = handler {
                    validate_span(source, self.span, handler.span)?;
                    if let Some(param) = &handler.param {
                        validate_expr(source, handler.span, param)?;
                    }
                    validate_stmt(source, handler.span, &handler.body)?;
                }
                if let Some(finalizer) = finalizer {
                    statement(finalizer)?;
                }
            }
            StmtKind::With { object, body } => {
                expression(object)?;
                statement(body)?;
            }
            StmtKind::Variable { declarations, .. } => {
                validate_declarations(source, self.span, declarations)?
            }
        }
        Ok(())
    }
}
