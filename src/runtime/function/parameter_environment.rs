//! Prepared FunctionDeclarationInstantiation data for parameter expressions.
use super::*;
use crate::semantics::{DeclarationScope, bound_names, var_declared_names};

#[derive(Debug)]
pub(super) struct Plan {
    vars: Box<[(JsString, bool)]>,
}
impl Plan {
    pub(super) fn prepare(function: &Function) -> Option<Self> {
        if !function.parameters.iter().any(contains_expression) {
            return None;
        }
        let FunctionBody::Block(body) = &function.body else {
            return Some(Self { vars: Box::new([]) });
        };
        let StmtKind::Block(statements) = &body.kind else {
            return Some(Self { vars: Box::new([]) });
        };
        let parameters: Vec<_> = function.parameters.iter().flat_map(bound_names).collect();
        let functions: Vec<_> = statements
            .iter()
            .filter_map(|stmt| {
                if let StmtKind::FunctionDeclaration(function) = &stmt.kind {
                    function.name.as_ref().and_then(|name| {
                        if let ExprKind::Identifier(name) = &name.kind {
                            Some(name.as_str())
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            })
            .collect();
        let mut vars = Vec::new();
        for name in var_declared_names(statements, DeclarationScope::Function) {
            let key = JsString::from(name.name.as_str());
            if vars.iter().any(|(existing, _)| existing == &key) {
                continue;
            }
            let copy = !functions.contains(&name.name.as_str())
                && (name.name == "arguments" || parameters.iter().any(|p| p.name == name.name));
            vars.push((key, copy));
        }
        Some(Self {
            vars: vars.into_boxed_slice(),
        })
    }
    pub(super) fn instantiate(&self, parameters: &Environment) -> RuntimeResult<Environment> {
        let body = Environment::declarative(parameters.clone());
        for (name, copy) in &self.vars {
            let value = if *copy && parameters.has_binding(name)? {
                parameters.get_binding_value(name, false)?
            } else {
                Value::Undefined
            };
            body.create_mutable_binding(name.clone(), false)?;
            body.initialize_binding(name, value)?;
        }
        Ok(body)
    }
}
fn contains_expression(binding: &Expr) -> bool {
    match &binding.kind {
        ExprKind::Assignment { .. } => true,
        ExprKind::Spread(target) => contains_expression(target),
        ExprKind::Array { elements, .. } => elements.iter().flatten().any(contains_expression),
        ExprKind::Object(properties) => properties
            .iter()
            .any(|p| p.computed || contains_expression(p.value.as_ref().unwrap_or(&p.key))),
        _ => false,
    }
}
