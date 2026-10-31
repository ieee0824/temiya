use super::*;

/// An activation cannot escape from simple parameter-to-property assignments.
/// Retain ordinary [[Set]], including inherited setters and strict failures.
#[derive(Debug)]
pub(super) struct Plan(Box<[(PropertyKey, usize)]>);

impl Plan {
    pub(super) fn prepare(function: &Function, kind: FunctionKind) -> Option<Self> {
        if kind != FunctionKind::Method(MethodKind::Constructor)
            || !function.strict
            || function.asynchronous
            || function.generator
        {
            return None;
        }
        let mut parameters = HashMap::new();
        for (index, parameter) in function.parameters.iter().enumerate() {
            let ExprKind::Identifier(name) = &parameter.kind else {
                return None;
            };
            if name == "arguments"
                || name == "eval"
                || parameters.insert(name.as_str(), index).is_some()
            {
                return None;
            }
        }
        let FunctionBody::Block(body) = &function.body else {
            return None;
        };
        let StmtKind::Block(statements) = &body.kind else {
            return None;
        };
        let assignments = statements
            .iter()
            .map(|statement| {
                let StmtKind::Expression(expression) = &statement.kind else {
                    return None;
                };
                let ExprKind::Assignment {
                    operator,
                    target,
                    value,
                } = &expression.kind
                else {
                    return None;
                };
                if operator != "=" {
                    return None;
                }
                let ExprKind::Member {
                    object,
                    property,
                    computed: false,
                    optional: false,
                } = &target.kind
                else {
                    return None;
                };
                if !matches!(object.kind, ExprKind::This) {
                    return None;
                }
                let ExprKind::Identifier(property) = &property.kind else {
                    return None;
                };
                let ExprKind::Identifier(parameter) = &value.kind else {
                    return None;
                };
                Some((
                    PropertyKey::from(property.as_str()),
                    *parameters.get(parameter.as_str())?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self(assignments.into_boxed_slice()))
    }

    pub(super) fn execute(&self, instance: &Object, arguments: &[Value]) -> RuntimeResult<()> {
        for (key, index) in &self.0 {
            let value = arguments.get(*index).cloned().unwrap_or(Value::Undefined);
            if !instance.set(key, value)? {
                return Err(RuntimeError::type_error("cannot assign to property"));
            }
        }
        Ok(())
    }
}
