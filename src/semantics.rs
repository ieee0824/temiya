//! Scope-aware early errors and reusable syntax-name queries.

use crate::ast::*;
use crate::lexer::{SourceGoal, Span};

mod check;

/// A decoded binding name and the syntax range that introduced it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundName {
    /// Unicode name without normalization.
    pub name: String,
    /// Original binding range.
    pub span: Span,
}

/// Declaration-list semantics differ at source/function and block boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeclarationScope {
    /// Script body: direct function declarations are var scoped.
    Script,
    /// Module body: imports and direct functions are lexical declarations.
    Module,
    /// Function body: direct function declarations are var scoped.
    Function,
    /// Block or switch body: direct functions are lexical declarations.
    Block,
}

/// Syntax features available through the structural `contains` query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxFeature {
    /// Await expressions.
    Await,
    /// Yield expressions.
    Yield,
    /// Calls whose callee is super.
    SuperCall,
    /// Member expressions whose object is super.
    SuperProperty,
    /// Any identifier syntax node named arguments, including bindings.
    Arguments,
    /// The new.target meta-property.
    NewTarget,
}

/// A static syntax error with its original source range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticError {
    /// Invalid syntax range.
    pub span: Span,
    /// Description of the violated constraint.
    pub message: &'static str,
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}
impl std::error::Error for SemanticError {}

/// Context for checking a parsed expression or source-body fragment.
#[derive(Clone, Copy, Debug, Default)]
pub struct SemanticOptions {
    /// Source goal; Module is always strict and reserves await in nested code.
    pub source_goal: SourceGoal,
    /// Whether the root fragment is strict.
    pub strict: bool,
    /// Whether return/new.target are permitted in the root fragment.
    pub in_function: bool,
    /// Whether await and for-await-of are permitted in the root fragment.
    pub asynchronous: bool,
    /// Whether yield is permitted in the root fragment.
    pub generator: bool,
}

/// Returns names introduced by a binding pattern, excluding default expressions.
/// Non-binding syntax is ignored; validation separately rejects invalid patterns.
#[must_use]
pub fn bound_names(pattern: &Expr) -> Vec<BoundName> {
    match &pattern.kind {
        ExprKind::Identifier(name) => vec![BoundName {
            name: name.clone(),
            span: pattern.span,
        }],
        ExprKind::Spread(pattern)
        | ExprKind::Assignment {
            target: pattern, ..
        } => bound_names(pattern),
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

fn name_of(expr: &Option<Expr>) -> Vec<BoundName> {
    expr.iter().flat_map(bound_names).collect()
}

/// Returns directly lexical names, without descending into nested scopes.
#[must_use]
pub fn lexically_declared_names(body: &[Stmt], scope: DeclarationScope) -> Vec<BoundName> {
    let mut names = vec![];
    for stmt in body {
        match &stmt.kind {
            StmtKind::Variable {
                kind: VariableKind::Let | VariableKind::Const,
                declarations,
            } => names.extend(declarations.iter().flat_map(|d| bound_names(&d.binding))),
            StmtKind::ClassDeclaration(class) => names.extend(name_of(&class.name)),
            StmtKind::FunctionDeclaration(function)
                if matches!(scope, DeclarationScope::Block | DeclarationScope::Module) =>
            {
                names.extend(name_of(&function.name))
            }
            StmtKind::ImportDeclaration(import) => {
                names.extend(import.specifiers.iter().flat_map(|s| bound_names(&s.local)))
            }
            StmtKind::ExportDeclaration(export) => match &export.kind {
                ExportKind::Declaration(stmt) => names.extend(lexically_declared_names(
                    std::slice::from_ref(stmt.as_ref()),
                    scope,
                )),
                ExportKind::DefaultDeclaration(stmt) => {
                    let declared =
                        lexically_declared_names(std::slice::from_ref(stmt.as_ref()), scope);
                    if declared.is_empty() {
                        names.push(BoundName {
                            name: "*default*".to_owned(),
                            span: stmt.span,
                        });
                    } else {
                        names.extend(declared);
                    }
                }
                ExportKind::DefaultExpression(expr) => names.push(BoundName {
                    name: "*default*".to_owned(),
                    span: expr.span,
                }),
                _ => {}
            },
            _ => {}
        }
    }
    names
}

/// Returns var names hoisted through statements, excluding nested function code.
/// Direct function declarations are included for Script/Function body scopes.
#[must_use]
pub fn var_declared_names(body: &[Stmt], scope: DeclarationScope) -> Vec<BoundName> {
    let mut names = vec![];
    for stmt in body {
        match &stmt.kind {
            StmtKind::Variable {
                kind: VariableKind::Var,
                declarations,
            } => names.extend(declarations.iter().flat_map(|d| bound_names(&d.binding))),
            StmtKind::FunctionDeclaration(function)
                if matches!(scope, DeclarationScope::Script | DeclarationScope::Function) =>
            {
                names.extend(name_of(&function.name))
            }
            StmtKind::Block(body) => {
                names.extend(var_declared_names(body, DeclarationScope::Block))
            }
            StmtKind::If {
                consequent,
                alternate,
                ..
            } => {
                names.extend(var_declared_names(
                    std::slice::from_ref(consequent.as_ref()),
                    DeclarationScope::Block,
                ));
                if let Some(alternate) = alternate {
                    names.extend(var_declared_names(
                        std::slice::from_ref(alternate.as_ref()),
                        DeclarationScope::Block,
                    ));
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Labeled { body, .. }
            | StmtKind::With { body, .. } => names.extend(var_declared_names(
                std::slice::from_ref(body.as_ref()),
                DeclarationScope::Block,
            )),
            StmtKind::For { init, body, .. } => {
                if let Some(init) = init {
                    names.extend(for_var_names(init));
                }
                names.extend(var_declared_names(
                    std::slice::from_ref(body.as_ref()),
                    DeclarationScope::Block,
                ));
            }
            StmtKind::ForEach { left, body, .. } => {
                names.extend(for_var_names(left));
                names.extend(var_declared_names(
                    std::slice::from_ref(body.as_ref()),
                    DeclarationScope::Block,
                ));
            }
            StmtKind::Switch { cases, .. } => {
                for case in cases {
                    names.extend(var_declared_names(
                        &case.consequent,
                        DeclarationScope::Block,
                    ));
                }
            }
            StmtKind::Try {
                block,
                handler,
                finalizer,
            } => {
                names.extend(var_declared_names(
                    std::slice::from_ref(block.as_ref()),
                    DeclarationScope::Block,
                ));
                if let Some(handler) = handler {
                    names.extend(var_declared_names(
                        std::slice::from_ref(handler.body.as_ref()),
                        DeclarationScope::Block,
                    ));
                }
                if let Some(finalizer) = finalizer {
                    names.extend(var_declared_names(
                        std::slice::from_ref(finalizer.as_ref()),
                        DeclarationScope::Block,
                    ));
                }
            }
            StmtKind::ExportDeclaration(export) => {
                if let ExportKind::Declaration(stmt) | ExportKind::DefaultDeclaration(stmt) =
                    &export.kind
                {
                    names.extend(var_declared_names(
                        std::slice::from_ref(stmt.as_ref()),
                        scope,
                    ));
                }
            }
            _ => {}
        }
    }
    names
}

fn for_var_names(init: &ForInit) -> Vec<BoundName> {
    if let ForInit::Variable {
        kind: VariableKind::Var,
        declarations,
    } = init
    {
        declarations
            .iter()
            .flat_map(|d| bound_names(&d.binding))
            .collect()
    } else {
        vec![]
    }
}

/// Searches all descendant syntax, including nested functions and binding names.
/// Scope-sensitive early-error checking uses `validate_program` instead.
#[must_use]
pub fn contains(expr: &Expr, feature: SyntaxFeature) -> bool {
    let mut found = false;
    expr.visit(&mut |node| {
        found |= match (&node.kind, feature) {
            (ExprKind::Unary { operator, .. }, SyntaxFeature::Await) => operator == "await",
            (ExprKind::Yield { .. }, SyntaxFeature::Yield) => true,
            (ExprKind::Call { callee, .. }, SyntaxFeature::SuperCall) => {
                matches!(callee.kind, ExprKind::Super)
            }
            (ExprKind::Member { object, .. }, SyntaxFeature::SuperProperty) => {
                matches!(object.kind, ExprKind::Super)
            }
            (ExprKind::Identifier(name), SyntaxFeature::Arguments) => name == "arguments",
            (ExprKind::MetaProperty { meta, property }, SyntaxFeature::NewTarget) => {
                meta == "new" && property == "target"
            }
            _ => false,
        }
    });
    found
}

/// Validates scopes, declarations, references, and contextual syntax in a source unit.
pub fn validate_program(program: &Program, options: SemanticOptions) -> Result<(), SemanticError> {
    check::program(program, options)
}

/// Validates one expression, including any nested function/class code.
pub fn validate_expression(expr: &Expr, options: SemanticOptions) -> Result<(), SemanticError> {
    check::expression(expr, options)
}
