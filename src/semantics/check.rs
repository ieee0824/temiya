use super::*;
use std::collections::HashSet;

#[derive(Clone)]
struct Context {
    options: SemanticOptions,
    strict: bool,
    in_function: bool,
    asynchronous: bool,
    generator: bool,
    parameters: bool,
    super_property: bool,
    super_call: bool,
    new_target: bool,
    forbid_arguments: bool,
    await_reserved: bool,
    loops: usize,
    switches: usize,
    labels: Vec<(String, bool)>,
    private_names: Vec<HashSet<String>>,
    module_items: bool,
    depth: usize,
}

fn error(span: Span, message: &'static str) -> SemanticError {
    SemanticError { span, message }
}

impl Context {
    fn new(options: SemanticOptions) -> Self {
        Self {
            options,
            strict: options.strict || options.source_goal == SourceGoal::Module,
            in_function: options.in_function,
            asynchronous: options.asynchronous
                || (options.source_goal == SourceGoal::Module && !options.in_function),
            generator: options.generator,
            parameters: false,
            super_property: false,
            super_call: false,
            new_target: options.in_function,
            forbid_arguments: false,
            await_reserved: false,
            loops: 0,
            switches: 0,
            labels: vec![],
            private_names: vec![],
            module_items: options.source_goal == SourceGoal::Module && !options.in_function,
            depth: 0,
        }
    }
    fn deeper(&self, span: Span) -> Result<Self, SemanticError> {
        if self.depth >= 128 {
            return Err(error(span, "static semantics nesting limit exceeded"));
        }
        let mut next = self.clone();
        next.depth += 1;
        Ok(next)
    }
    fn name(&self, name: &str, span: Span, binding: bool) -> Result<(), SemanticError> {
        if always_reserved(name)
            || ((self.asynchronous
                || self.await_reserved
                || self.options.source_goal == SourceGoal::Module)
                && name == "await")
            || (self.generator && name == "yield")
            || (self.strict && strict_reserved(name))
            || (binding && self.strict && matches!(name, "eval" | "arguments"))
            || (self.forbid_arguments && name == "arguments")
        {
            return Err(error(span, "invalid identifier in this context"));
        }
        Ok(())
    }
    fn private(&self, name: &str, span: Span) -> Result<(), SemanticError> {
        if !self
            .private_names
            .iter()
            .rev()
            .any(|names| names.contains(name))
        {
            return Err(error(span, "undeclared private name"));
        }
        Ok(())
    }
}

pub(super) fn program(program: &Program, options: SemanticOptions) -> Result<(), SemanticError> {
    let mut context = Context::new(options);
    context.strict |= program.strict;
    let scope = if options.in_function {
        DeclarationScope::Function
    } else if options.source_goal == SourceGoal::Module {
        DeclarationScope::Module
    } else {
        DeclarationScope::Script
    };
    list(&program.body, scope, &context, &[])?;
    if scope == DeclarationScope::Module {
        exports(&program.body)?;
    }
    Ok(())
}

pub(super) fn expression(expr: &Expr, options: SemanticOptions) -> Result<(), SemanticError> {
    check_expr(expr, &Context::new(options))
}

fn scope(
    body: &[&Stmt],
    kind: DeclarationScope,
    parameters: &[BoundName],
) -> Result<(), SemanticError> {
    let lexical: Vec<_> = body
        .iter()
        .flat_map(|stmt| lexically_declared_names(std::slice::from_ref(*stmt), kind))
        .collect();
    let vars: Vec<_> = body
        .iter()
        .flat_map(|stmt| var_declared_names(std::slice::from_ref(*stmt), kind))
        .collect();
    let mut names = HashSet::new();
    for name in &lexical {
        if !names.insert(&name.name) {
            return Err(error(name.span, "duplicate lexical declaration"));
        }
    }
    for name in vars.iter().chain(parameters) {
        if names.contains(&name.name) {
            return Err(error(
                name.span,
                "lexical declaration conflicts with a var or parameter",
            ));
        }
    }
    Ok(())
}

fn list(
    body: &[Stmt],
    kind: DeclarationScope,
    context: &Context,
    parameters: &[BoundName],
) -> Result<(), SemanticError> {
    scope(&body.iter().collect::<Vec<_>>(), kind, parameters)?;
    let mut context = context.clone();
    context.module_items = kind == DeclarationScope::Module;
    for stmt in body {
        check_stmt(stmt, &context)?;
    }
    Ok(())
}

fn unique(names: &[BoundName]) -> Result<(), SemanticError> {
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(&name.name) {
            return Err(error(name.span, "duplicate binding name"));
        }
    }
    Ok(())
}

fn check_binding(pattern: &Expr, context: &Context, assignment: bool) -> Result<(), SemanticError> {
    let context = context.deeper(pattern.span)?;
    match &pattern.kind {
        ExprKind::Identifier(name) => {
            if assignment && context.strict && matches!(name.as_str(), "eval" | "arguments") {
                return Err(error(pattern.span, "invalid strict assignment target"));
            }
            context.name(name, pattern.span, !assignment)
        }
        ExprKind::Member { .. } if assignment => check_expr(pattern, &context),
        ExprKind::Parenthesized(inner)
            if assignment
                && matches!(
                    inner.kind,
                    ExprKind::Identifier(_) | ExprKind::Member { .. } | ExprKind::Parenthesized(_)
                ) =>
        {
            check_binding(inner, &context, true)
        }
        ExprKind::Assignment {
            operator,
            target,
            value,
        } if operator == "=" => {
            check_binding(target, &context, assignment)?;
            check_expr(value, &context)
        }
        ExprKind::Spread(target) => {
            if matches!(target.kind, ExprKind::Assignment { .. }) {
                return Err(error(target.span, "rest cannot have a default"));
            }
            check_binding(target, &context, assignment)
        }
        ExprKind::Array {
            elements,
            trailing_comma,
        } => {
            for (i, element) in elements.iter().enumerate() {
                if let Some(element) = element {
                    if matches!(element.kind, ExprKind::Spread(_)) && i + 1 != elements.len() {
                        return Err(error(element.span, "rest must be the last element"));
                    }
                    if *trailing_comma && matches!(element.kind, ExprKind::Spread(_)) {
                        return Err(error(element.span, "trailing comma after rest element"));
                    }
                    check_binding(element, &context, assignment)?;
                }
            }
            Ok(())
        }
        ExprKind::Object(properties) => {
            for (i, property) in properties.iter().enumerate() {
                if property.spread && i + 1 != properties.len() {
                    return Err(error(property.span, "rest must be the last property"));
                }
                if property.spread && property.comma_after {
                    return Err(error(property.span, "trailing comma after object rest"));
                }
                if property.spread
                    && (property.value.is_some()
                        || (!assignment && !matches!(property.key.kind, ExprKind::Identifier(_)))
                        || (assignment
                            && !matches!(
                                property.key.kind,
                                ExprKind::Identifier(_) | ExprKind::Member { .. }
                            )))
                {
                    return Err(error(property.span, "invalid object rest target"));
                }
                if property.computed {
                    check_expr(&property.key, &context)?;
                }
                check_binding(
                    property.value.as_ref().unwrap_or(&property.key),
                    &context,
                    assignment,
                )?;
            }
            Ok(())
        }
        _ => Err(error(pattern.span, "invalid binding or assignment pattern")),
    }
}

fn declarations(
    declarations: &[VariableDeclarator],
    context: &Context,
) -> Result<(), SemanticError> {
    for declaration in declarations {
        check_binding(&declaration.binding, context, false)?;
        if let Some(init) = &declaration.init {
            check_expr(init, context)?;
        }
    }
    Ok(())
}

fn check_stmt(stmt: &Stmt, context: &Context) -> Result<(), SemanticError> {
    let context = context.deeper(stmt.span)?;
    match &stmt.kind {
        StmtKind::Empty | StmtKind::Debugger => Ok(()),
        StmtKind::Expression(expr) | StmtKind::Throw(expr) => check_expr(expr, &context),
        StmtKind::Return(value) => {
            if !context.in_function {
                return Err(error(stmt.span, "return outside a function"));
            }
            if let Some(value) = value {
                check_expr(value, &context)?;
            }
            Ok(())
        }
        StmtKind::Block(body) => list(body, DeclarationScope::Block, &context, &[]),
        StmtKind::Variable {
            declarations: decls,
            ..
        } => declarations(decls, &context),
        StmtKind::If {
            test,
            consequent,
            alternate,
        } => {
            check_expr(test, &context)?;
            single_statement(consequent, &context)?;
            if let Some(alternate) = alternate {
                single_statement(alternate, &context)?;
            }
            Ok(())
        }
        StmtKind::While { test, body } | StmtKind::DoWhile { test, body } => {
            check_expr(test, &context)?;
            let mut nested = context.clone();
            nested.loops += 1;
            single_statement(body, &nested)
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => {
            if let Some(init) = init {
                check_init(init, &context, false)?;
                loop_lexicals(init, body)?;
            }
            if let Some(test) = test {
                check_expr(test, &context)?;
            }
            if let Some(update) = update {
                check_expr(update, &context)?;
            }
            let mut nested = context.clone();
            nested.loops += 1;
            single_statement(body, &nested)
        }
        StmtKind::ForEach {
            left,
            right,
            body,
            asynchronous,
            ..
        } => {
            if *asynchronous && !context.asynchronous {
                return Err(error(stmt.span, "for-await outside an async context"));
            }
            check_init(left, &context, true)?;
            loop_lexicals(left, body)?;
            check_expr(right, &context)?;
            let mut nested = context.clone();
            nested.loops += 1;
            single_statement(body, &nested)
        }
        StmtKind::Labeled { label, body } => {
            context.name(label, stmt.span, false)?;
            if context.labels.iter().any(|(name, _)| name == label) {
                return Err(error(stmt.span, "duplicate active label"));
            }
            let mut nested = context.clone();
            nested.labels.push((label.clone(), iteration(body)));
            single_statement(body, &nested)
        }
        StmtKind::Break(label) | StmtKind::Continue(label) => {
            let continuation = matches!(stmt.kind, StmtKind::Continue(_));
            if let Some(label) = label {
                if !context
                    .labels
                    .iter()
                    .any(|(name, loop_)| name == label && (!continuation || *loop_))
                {
                    return Err(error(stmt.span, "invalid control-flow label"));
                }
            } else if context.loops == 0 && (continuation || context.switches == 0) {
                return Err(error(stmt.span, "invalid break or continue context"));
            }
            Ok(())
        }
        StmtKind::Switch {
            discriminant,
            cases,
        } => {
            check_expr(discriminant, &context)?;
            let body: Vec<_> = cases
                .iter()
                .flat_map(|case| case.consequent.iter())
                .collect();
            scope(&body, DeclarationScope::Block, &[])?;
            let mut nested = context.clone();
            nested.switches += 1;
            nested.module_items = false;
            let mut default = false;
            for case in cases {
                if let Some(test) = &case.test {
                    check_expr(test, &nested)?;
                } else {
                    if default {
                        return Err(error(case.span, "duplicate default clause"));
                    }
                    default = true;
                }
                for stmt in &case.consequent {
                    check_stmt(stmt, &nested)?;
                }
            }
            Ok(())
        }
        StmtKind::Try {
            block,
            handler,
            finalizer,
        } => {
            check_stmt(block, &context)?;
            if let Some(handler) = handler {
                if let Some(param) = &handler.param {
                    check_binding(param, &context, false)?;
                    let names = bound_names(param);
                    unique(&names)?;
                    if let StmtKind::Block(body) = &handler.body.kind {
                        let mut conflicts = lexically_declared_names(body, DeclarationScope::Block);
                        if !matches!(param.kind, ExprKind::Identifier(_)) {
                            conflicts.extend(var_declared_names(body, DeclarationScope::Block));
                        }
                        for name in conflicts {
                            if names.iter().any(|parameter| parameter.name == name.name) {
                                return Err(error(
                                    name.span,
                                    "catch binding conflicts with a declaration",
                                ));
                            }
                        }
                    }
                }
                check_stmt(&handler.body, &context)?;
            }
            if let Some(finalizer) = finalizer {
                check_stmt(finalizer, &context)?;
            }
            Ok(())
        }
        StmtKind::With { object, body } => {
            if context.strict {
                return Err(error(stmt.span, "with in strict code"));
            }
            check_expr(object, &context)?;
            single_statement(body, &context)
        }
        StmtKind::FunctionDeclaration(function) => {
            if let Some(name) = &function.name {
                check_binding(name, &context, false)?;
            }
            check_function(function, &context, false, None, true)
        }
        StmtKind::ClassDeclaration(class) => check_class(class, &context),
        StmtKind::ImportDeclaration(import) => {
            if !context.module_items {
                return Err(error(stmt.span, "import outside module top level"));
            }
            for specifier in &import.specifiers {
                check_binding(&specifier.local, &context, false)?;
            }
            Ok(())
        }
        StmtKind::ExportDeclaration(export) => {
            if !context.module_items {
                return Err(error(stmt.span, "export outside module top level"));
            }
            match &export.kind {
                ExportKind::Declaration(stmt) | ExportKind::DefaultDeclaration(stmt) => {
                    check_stmt(stmt, &context)
                }
                ExportKind::DefaultExpression(expr) => check_expr(expr, &context),
                _ => Ok(()),
            }
        }
    }
}

fn single_statement(stmt: &Stmt, context: &Context) -> Result<(), SemanticError> {
    if matches!(
        stmt.kind,
        StmtKind::Variable {
            kind: VariableKind::Let | VariableKind::Const,
            ..
        } | StmtKind::ClassDeclaration(_)
            | StmtKind::FunctionDeclaration(_)
    ) {
        return Err(error(
            stmt.span,
            "declaration in a single-statement position",
        ));
    }
    let mut context = context.clone();
    context.module_items = false;
    check_stmt(stmt, &context)
}

fn iteration(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::While { .. }
        | StmtKind::DoWhile { .. }
        | StmtKind::For { .. }
        | StmtKind::ForEach { .. } => true,
        StmtKind::Labeled { body, .. } => iteration(body),
        _ => false,
    }
}

fn check_init(init: &ForInit, context: &Context, iteration: bool) -> Result<(), SemanticError> {
    match init {
        ForInit::Expression(expr) => {
            if iteration {
                check_binding(expr, context, true)
            } else {
                check_expr(expr, context)
            }
        }
        ForInit::Variable {
            declarations: decls,
            kind,
        } => {
            declarations(decls, context)?;
            if iteration && decls.len() != 1 {
                return Err(error(
                    decls.first().map_or(Span { start: 0, end: 0 }, |d| d.span),
                    "iteration requires one binding",
                ));
            }
            for decl in decls {
                if !iteration
                    && (*kind == VariableKind::Const
                        || !matches!(decl.binding.kind, ExprKind::Identifier(_)))
                    && decl.init.is_none()
                {
                    return Err(error(decl.span, "binding requires an initializer"));
                }
            }
            Ok(())
        }
    }
}

fn loop_lexicals(init: &ForInit, body: &Stmt) -> Result<(), SemanticError> {
    if let ForInit::Variable {
        kind: VariableKind::Let | VariableKind::Const,
        declarations,
    } = init
    {
        let names: Vec<_> = declarations
            .iter()
            .flat_map(|d| bound_names(&d.binding))
            .collect();
        unique(&names)?;
        for var in var_declared_names(std::slice::from_ref(body), DeclarationScope::Block) {
            if names.iter().any(|name| name.name == var.name) {
                return Err(error(
                    var.span,
                    "loop binding conflicts with a var declaration",
                ));
            }
        }
    }
    Ok(())
}

fn check_expr(expr: &Expr, context: &Context) -> Result<(), SemanticError> {
    let context = context.deeper(expr.span)?;
    match &expr.kind {
        ExprKind::Identifier(name) => context.name(name, expr.span, false),
        ExprKind::PrivateIdentifier(_) => Err(error(
            expr.span,
            "private identifier is not a standalone expression",
        )),
        ExprKind::Literal(Literal::String {
            legacy_escape: true,
            ..
        }) if context.strict => Err(error(expr.span, "legacy string in strict code")),
        ExprKind::Literal(Literal::Number {
            kind: NumberKind::LegacyOctal | NumberKind::LegacyDecimal,
            ..
        }) if context.strict => Err(error(expr.span, "legacy number in strict code")),
        ExprKind::Literal(_) | ExprKind::This => Ok(()),
        ExprKind::Super => Err(error(expr.span, "standalone super expression")),
        ExprKind::Parenthesized(inner) | ExprKind::Spread(inner) => check_expr(inner, &context),
        ExprKind::Unary { operator, argument } => {
            if operator == "await" && (!context.asynchronous || context.parameters) {
                return Err(error(expr.span, "await in an invalid context"));
            }
            if operator == "delete" && context.strict && identifier(argument).is_some() {
                return Err(error(
                    argument.span,
                    "cannot delete an identifier in strict code",
                ));
            }
            check_expr(argument, &context)
        }
        ExprKind::Yield { argument, .. } => {
            if !context.generator || context.parameters {
                return Err(error(expr.span, "yield in an invalid context"));
            }
            if let Some(argument) = argument {
                check_expr(argument, &context)?;
            }
            Ok(())
        }
        ExprKind::Update { argument, .. } => {
            strict_target(argument, &context)?;
            check_binding(argument, &context, true)
        }
        ExprKind::Assignment { target, value, .. } => {
            strict_target(target, &context)?;
            check_binding(target, &context, true)?;
            check_expr(value, &context)
        }
        ExprKind::Binary {
            operator,
            left,
            right,
        } => {
            if let ExprKind::PrivateIdentifier(name) = &left.kind {
                if operator != "in" {
                    return Err(error(left.span, "private brand check requires in"));
                }
                context.private(name, left.span)?;
            } else {
                check_expr(left, &context)?;
            }
            check_expr(right, &context)
        }
        ExprKind::Conditional {
            test,
            consequent,
            alternate,
        } => {
            check_expr(test, &context)?;
            check_expr(consequent, &context)?;
            check_expr(alternate, &context)
        }
        ExprKind::Sequence(items) => {
            for item in items {
                check_expr(item, &context)?;
            }
            Ok(())
        }
        ExprKind::Array { elements, .. } => {
            for item in elements.iter().flatten() {
                check_expr(item, &context)?;
            }
            Ok(())
        }
        ExprKind::Object(properties) => {
            let mut proto = false;
            for property in properties {
                if property.computed {
                    check_expr(&property.key, &context)?;
                }
                if let Some(value) = &property.value {
                    if property.shorthand {
                        return Err(error(
                            property.span,
                            "initialized shorthand requires an assignment pattern",
                        ));
                    }
                    if !property.computed
                        && !property.spread
                        && !matches!(value.kind, ExprKind::Method { .. })
                        && property_name(&property.key).as_deref() == Some("__proto__")
                    {
                        if proto {
                            return Err(error(
                                property.key.span,
                                "duplicate __proto__ data property",
                            ));
                        }
                        proto = true;
                    }
                    check_expr(value, &context)?;
                } else if property.spread || !property.computed {
                    check_expr(&property.key, &context)?;
                }
            }
            Ok(())
        }
        ExprKind::Member {
            object,
            property,
            computed,
            ..
        } => {
            if matches!(object.kind, ExprKind::Super) {
                if !context.super_property {
                    return Err(error(object.span, "super property outside a method"));
                }
            } else {
                check_expr(object, &context)?;
            }
            if *computed {
                check_expr(property, &context)?;
            } else if let ExprKind::PrivateIdentifier(name) = &property.kind {
                if matches!(object.kind, ExprKind::Super) {
                    return Err(error(property.span, "private super property"));
                }
                context.private(name, property.span)?;
            }
            Ok(())
        }
        ExprKind::Call {
            callee, arguments, ..
        } => {
            if matches!(callee.kind, ExprKind::Super) {
                if !context.super_call {
                    return Err(error(
                        callee.span,
                        "super call outside a derived constructor",
                    ));
                }
            } else {
                check_expr(callee, &context)?;
            }
            for argument in arguments {
                check_expr(argument, &context)?;
            }
            Ok(())
        }
        ExprKind::New { callee, arguments } => {
            check_expr(callee, &context)?;
            for argument in arguments {
                check_expr(argument, &context)?;
            }
            Ok(())
        }
        ExprKind::ImportCall { specifier, options } => {
            check_expr(specifier, &context)?;
            if let Some(options) = options {
                check_expr(options, &context)?;
            }
            Ok(())
        }
        ExprKind::MetaProperty { meta, property } => {
            if (meta == "new" && property == "target" && context.new_target)
                || (meta == "import"
                    && property == "meta"
                    && context.options.source_goal == SourceGoal::Module)
            {
                Ok(())
            } else {
                Err(error(expr.span, "meta-property in an invalid context"))
            }
        }
        ExprKind::Template {
            tag,
            quasis,
            expressions,
        } => {
            if let Some(tag) = tag {
                check_expr(tag, &context)?;
            } else if quasis.iter().any(|q| q.cooked.is_none()) {
                return Err(error(expr.span, "invalid untagged template escape"));
            }
            for expression in expressions {
                check_expr(expression, &context)?;
            }
            Ok(())
        }
        ExprKind::Function(function) => check_function(function, &context, false, None, false),
        ExprKind::Arrow(function) => check_function(function, &context, true, None, false),
        ExprKind::Method { function, kind } => {
            check_function(function, &context, false, Some((*kind, false)), false)
        }
        ExprKind::Class(class) => check_class(class, &context),
    }
}

fn identifier(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name),
        ExprKind::Parenthesized(inner) => identifier(inner),
        _ => None,
    }
}
fn strict_target(expr: &Expr, context: &Context) -> Result<(), SemanticError> {
    if context.strict && matches!(identifier(expr), Some("eval" | "arguments")) {
        return Err(error(expr.span, "invalid strict assignment target"));
    }
    Ok(())
}

fn check_function(
    function: &Function,
    outer: &Context,
    arrow: bool,
    method: Option<(MethodKind, bool)>,
    declaration: bool,
) -> Result<(), SemanticError> {
    let mut context = outer.clone();
    context.strict |= function.strict;
    context.in_function = true;
    context.asynchronous = function.asynchronous;
    context.generator = function.generator;
    context.parameters = true;
    context.await_reserved = false;
    context.module_items = false;
    context.loops = 0;
    context.switches = 0;
    context.labels.clear();
    if !arrow {
        context.super_property = method.is_some();
        context.super_call =
            method.is_some_and(|(kind, derived)| kind == MethodKind::Constructor && derived);
        context.new_target = true;
        context.forbid_arguments = false;
    }
    if let Some(name) = &function.name {
        let mut named = context.clone();
        if declaration {
            named.asynchronous = outer.asynchronous;
            named.generator = outer.generator;
        }
        check_binding(name, &named, false)?;
    }
    let mut parameter_context = context.clone();
    if arrow {
        parameter_context.asynchronous |= outer.asynchronous;
        parameter_context.generator |= outer.generator;
        parameter_context.await_reserved |= outer.await_reserved;
    }
    let mut names = vec![];
    for parameter in &function.parameters {
        check_binding(parameter, &parameter_context, false)?;
        names.extend(bound_names(parameter));
    }
    let simple = function
        .parameters
        .iter()
        .all(|p| matches!(p.kind, ExprKind::Identifier(_)));
    if arrow || method.is_some() || context.strict || !simple {
        unique(&names)?;
    }
    context.parameters = false;
    match &function.body {
        FunctionBody::Expression(expr) => check_expr(expr, &context),
        FunctionBody::Block(block) => {
            if let StmtKind::Block(body) = &block.kind {
                list(body, DeclarationScope::Function, &context, &names)
            } else {
                Err(error(block.span, "function body must be a block"))
            }
        }
    }
}

fn check_class(class: &Class, outer: &Context) -> Result<(), SemanticError> {
    let mut context = outer.clone();
    context.strict = true;
    if let Some(name) = &class.name {
        check_binding(name, &context, false)?;
    }
    if let Some(super_class) = &class.super_class {
        check_expr(super_class, &context)?;
    }
    let private_names = class
        .elements
        .iter()
        .filter_map(|element| element.key.as_ref())
        .filter_map(|key| {
            if let ExprKind::PrivateIdentifier(name) = &key.kind {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();
    context.private_names.push(private_names);
    for element in &class.elements {
        if element.computed {
            if let Some(key) = &element.key {
                check_expr(key, &context)?;
            }
        }
        match &element.kind {
            ClassElementKind::Method { function, kind } => check_function(
                function,
                &context,
                false,
                Some((*kind, class.super_class.is_some())),
                false,
            )?,
            ClassElementKind::Field(value) => {
                if let Some(value) = value {
                    let mut field = field_context(&context, false);
                    field.parameters = false;
                    check_expr(value, &field)?;
                }
            }
            ClassElementKind::StaticBlock(block) => {
                check_stmt(block, &field_context(&context, true))?
            }
        }
    }
    Ok(())
}

fn field_context(outer: &Context, static_block: bool) -> Context {
    let mut context = outer.clone();
    context.in_function = false;
    context.asynchronous = false;
    context.generator = false;
    context.parameters = false;
    context.await_reserved = static_block;
    context.super_property = true;
    context.super_call = false;
    context.new_target = true;
    context.forbid_arguments = true;
    context.module_items = false;
    context.loops = 0;
    context.switches = 0;
    context.labels.clear();
    context
}

fn property_name(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name.clone()),
        ExprKind::Literal(Literal::String { value, .. }) => String::from_utf16(value).ok(),
        _ => None,
    }
}
fn always_reserved(name: &str) -> bool {
    matches!(
        name,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
    )
}
fn strict_reserved(name: &str) -> bool {
    matches!(
        name,
        "implements"
            | "interface"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "let"
            | "yield"
    )
}

fn exports(body: &[Stmt]) -> Result<(), SemanticError> {
    let bindings: HashSet<_> = lexically_declared_names(body, DeclarationScope::Module)
        .into_iter()
        .chain(var_declared_names(body, DeclarationScope::Module))
        .map(|name| name.name.encode_utf16().collect::<Vec<_>>())
        .collect();
    let mut exported = HashSet::new();
    for stmt in body {
        if let StmtKind::ExportDeclaration(export) = &stmt.kind {
            let names = match &export.kind {
                ExportKind::Named {
                    specifiers,
                    request,
                } => {
                    if request.is_none() {
                        for specifier in specifiers {
                            if !bindings.contains(&specifier.local) {
                                return Err(error(
                                    specifier.local_span,
                                    "export refers to an undeclared local binding",
                                ));
                            }
                        }
                    }
                    specifiers
                        .iter()
                        .map(|s| (s.exported.clone(), s.span))
                        .collect::<Vec<_>>()
                }
                ExportKind::All { exported, .. } => exported
                    .iter()
                    .map(|name| (name.clone(), export.span))
                    .collect(),
                ExportKind::DefaultExpression(_) | ExportKind::DefaultDeclaration(_) => {
                    vec![("default".encode_utf16().collect(), export.span)]
                }
                ExportKind::Declaration(stmt) => lexically_declared_names(
                    std::slice::from_ref(stmt.as_ref()),
                    DeclarationScope::Module,
                )
                .into_iter()
                .chain(var_declared_names(
                    std::slice::from_ref(stmt.as_ref()),
                    DeclarationScope::Module,
                ))
                .map(|name| (name.name.encode_utf16().collect(), name.span))
                .collect(),
            };
            for (name, span) in names {
                if !exported.insert(name) {
                    return Err(error(span, "duplicate exported name"));
                }
            }
        }
    }
    Ok(())
}
