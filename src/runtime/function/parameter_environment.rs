//! Prepared FunctionDeclarationInstantiation data for parameter expressions.
use super::*;
use crate::semantics::{DeclarationScope, bound_names, var_declared_names};
use std::collections::HashSet;

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
        let parameters: HashSet<_> = function
            .parameters
            .iter()
            .flat_map(bound_names)
            .map(|name| name.name)
            .collect();
        let functions: HashSet<_> = statements
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
        let declared = var_declared_names(statements, DeclarationScope::Function);
        let mut seen = HashSet::new();
        let mut vars = Vec::new();
        for name in &declared {
            if !seen.insert(name.name.as_str()) {
                continue;
            }
            let key = JsString::from(name.name.as_str());
            let copy = !functions.contains(name.name.as_str())
                && (name.name == "arguments" || parameters.contains(name.name.as_str()));
            vars.push((key, copy));
        }
        Some(Self {
            vars: vars.into_boxed_slice(),
        })
    }
    pub(super) fn instantiate(&self, parameters: &Environment) -> RuntimeResult<Environment> {
        // Declaration preparation proves the name is unique. Publish its
        // copied value with the new record, tracing the initial edge at
        // allocation instead of creating and then mutating an empty record.
        if let [(name, copy)] = &*self.vars {
            let value = if *copy && parameters.has_binding(name)? {
                parameters.get_binding_value(name, false)?
            } else {
                Value::Undefined
            };
            return Environment::declarative_with_verified_initial_value(
                parameters.clone(),
                name.clone(),
                true,
                value,
            );
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Object, Runtime};

    #[test]
    fn copied_single_body_cell_survives_allocation_collection_and_stays_independent() {
        let runtime = Runtime::new();
        let parameters = Environment::declarative_in(&runtime);
        let name = JsString::from("value");
        let object = Object::new_in(&runtime);
        let weak = object.downgrade();
        parameters
            .create_mutable_binding(name.clone(), false)
            .unwrap();
        parameters
            .initialize_binding(&name, Value::Object(object))
            .unwrap();
        let plan = Plan {
            vars: vec![(name.clone(), true)].into_boxed_slice(),
        };
        runtime.set_allocation_collection_interval(1);
        let body = plan.instantiate(&parameters).unwrap();
        runtime.collect_garbage().unwrap();
        assert!(matches!(
            body.get_binding_value(&name, false).unwrap(),
            Value::Object(_)
        ));
        body.set_mutable_binding(&name, Value::Number(42.0), false)
            .unwrap();
        assert!(matches!(
            parameters.get_binding_value(&name, false).unwrap(),
            Value::Object(_)
        ));
        assert!(
            body.get_binding_value(&name, false)
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        runtime.verify_migration_barriers().unwrap();
        drop(body);
        drop(parameters);
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }
}
