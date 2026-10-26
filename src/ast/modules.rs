use super::*;

/// A statically requested module and its import attributes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleRequest {
    /// Range from the module string through its optional with clause.
    pub span: Span,
    /// Decoded module specifier in UTF-16 code units.
    pub specifier: Vec<u16>,
    /// Attributes in source order (canonicalized by `Program::module_requests`).
    pub attributes: Vec<ImportAttribute>,
}

/// A static import attribute with decoded UTF-16 key and value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportAttribute {
    /// Key through value range.
    pub span: Span,
    /// Decoded attribute key.
    pub key: Vec<u16>,
    /// Decoded string attribute value.
    pub value: Vec<u16>,
}

/// The imported binding of a named/default or namespace import.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportName {
    /// A named export; the default import uses the name `default`.
    Name(Vec<u16>),
    /// A namespace import.
    Namespace,
}

/// One local binding introduced by a static import.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportSpecifier {
    /// Entire specifier range.
    pub span: Span,
    /// Requested exported name or namespace.
    pub imported: ImportName,
    /// Local binding identifier.
    pub local: Expr,
}

/// A static import declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportDeclaration {
    /// Entire declaration range.
    pub span: Span,
    /// Requested module and attributes.
    pub request: ModuleRequest,
    /// Bindings; empty for a side-effect-only import.
    pub specifiers: Vec<ImportSpecifier>,
}

/// A named export, either local or re-exported from another module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportSpecifier {
    /// Entire export specifier range.
    pub span: Span,
    /// Original local/exported source name's range.
    pub local_span: Span,
    /// Local binding name, or imported name for re-exports.
    pub local: Vec<u16>,
    /// Public exported name.
    pub exported: Vec<u16>,
}

/// An export declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportDeclaration {
    /// Entire declaration range.
    pub span: Span,
    /// Export form.
    pub kind: ExportKind,
}

/// Static export forms, preserving default declarations versus expressions.
#[derive(Clone, Debug, PartialEq)]
pub enum ExportKind {
    /// Named local exports or re-exports.
    Named {
        /// Named entries in source order.
        specifiers: Vec<ExportSpecifier>,
        /// Optional re-export module.
        request: Option<ModuleRequest>,
    },
    /// Star export or namespace re-export.
    All {
        /// Exported namespace name, absent for `export *`.
        exported: Option<Vec<u16>>,
        /// Re-export module.
        request: ModuleRequest,
    },
    /// A variable, function, or class declaration exported by name.
    Declaration(Box<Stmt>),
    /// A default function or class declaration, possibly anonymous.
    DefaultDeclaration(Box<Stmt>),
    /// A default expression.
    DefaultExpression(Expr),
}

impl Program {
    /// Returns unique static module requests in first-occurrence order.
    ///
    /// Attributes are sorted by UTF-16 key for identity comparisons. Source
    /// spans refer to the first occurrence. Dynamic imports are excluded.
    /// This does not resolve, fetch, link, or evaluate modules.
    #[must_use]
    pub fn module_requests(&self) -> Vec<ModuleRequest> {
        let mut requests: Vec<ModuleRequest> = vec![];
        for stmt in &self.body {
            let request = match &stmt.kind {
                StmtKind::ImportDeclaration(import) => Some(&import.request),
                StmtKind::ExportDeclaration(export) => match &export.kind {
                    ExportKind::Named { request, .. } => request.as_ref(),
                    ExportKind::All { request, .. } => Some(request),
                    _ => None,
                },
                _ => None,
            };
            if let Some(request) = request {
                let mut request = request.clone();
                request.attributes.sort_by(|a, b| a.key.cmp(&b.key));
                if !requests.iter().any(|r| {
                    r.specifier == request.specifier
                        && r.attributes
                            .iter()
                            .map(|a| (&a.key, &a.value))
                            .eq(request.attributes.iter().map(|a| (&a.key, &a.value)))
                }) {
                    requests.push(request);
                }
            }
        }
        requests
    }
}

impl ModuleRequest {
    fn validate(&self, source: &SourceText<'_>, parent: Span) -> Result<(), AstError> {
        validate_span(source, parent, self.span)?;
        for attribute in &self.attributes {
            validate_span(source, self.span, attribute.span)?;
        }
        Ok(())
    }
}

impl ImportDeclaration {
    pub(super) fn validate(&self, source: &SourceText<'_>, parent: Span) -> Result<(), AstError> {
        validate_span(source, parent, self.span)?;
        self.request.validate(source, self.span)?;
        for specifier in &self.specifiers {
            validate_span(source, self.span, specifier.span)?;
            validate_expr(source, specifier.span, &specifier.local)?;
        }
        Ok(())
    }
}

impl ExportDeclaration {
    pub(super) fn validate(&self, source: &SourceText<'_>, parent: Span) -> Result<(), AstError> {
        validate_span(source, parent, self.span)?;
        match &self.kind {
            ExportKind::Named {
                request,
                specifiers,
            } => {
                if let Some(request) = request {
                    request.validate(source, self.span)?;
                }
                for specifier in specifiers {
                    validate_span(source, self.span, specifier.span)?;
                    validate_span(source, specifier.span, specifier.local_span)?;
                }
            }
            ExportKind::All { request, .. } => request.validate(source, self.span)?,
            ExportKind::Declaration(stmt) | ExportKind::DefaultDeclaration(stmt) => {
                validate_stmt(source, self.span, stmt)?
            }
            ExportKind::DefaultExpression(expr) => validate_expr(source, self.span, expr)?,
        }
        Ok(())
    }
}
