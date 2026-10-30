//! Complete-body delegation plans whose activation cannot be observed.
use super::*;
use crate::runtime::{Reference, ReferencedName};

#[derive(Debug)]
pub(super) struct Plan {
    name: JsString,
    arguments: Box<[usize]>,
}

impl Plan {
    pub(super) fn prepare(function: &Function, kind: FunctionKind) -> Option<Self> {
        if function.asynchronous
            || function.generator
            || !matches!(
                kind,
                FunctionKind::Arrow
                    | FunctionKind::Method(MethodKind::Method | MethodKind::Get | MethodKind::Set)
            )
        {
            return None;
        }
        let expression = match &function.body {
            FunctionBody::Expression(expression) => expression.as_ref(),
            FunctionBody::Block(body) => {
                let StmtKind::Block(statements) = &body.kind else {
                    return None;
                };
                let [statement] = statements.as_slice() else {
                    return None;
                };
                let StmtKind::Return(Some(expression)) = &statement.kind else {
                    return None;
                };
                expression
            }
        };
        let ExprKind::Call {
            callee,
            arguments,
            optional: false,
        } = &expression.kind
        else {
            return None;
        };
        let ExprKind::Member {
            object,
            property,
            computed: false,
            optional: false,
        } = &callee.kind
        else {
            return None;
        };
        if !matches!(object.kind, ExprKind::Super) {
            return None;
        }
        let ExprKind::Identifier(name) = &property.kind else {
            return None;
        };
        let mut parameters = HashMap::new();
        for (index, parameter) in function.parameters.iter().enumerate() {
            let ExprKind::Identifier(name) = &parameter.kind else {
                return None;
            };
            if name == "arguments" || name == "eval" {
                return None;
            }
            if parameters.insert(name.as_str(), index).is_some() {
                return None;
            }
        }
        if arguments.len() > crate::runtime::iterator::MAX_ARGUMENTS {
            return None;
        }
        let arguments = arguments
            .iter()
            .map(|argument| {
                let ExprKind::Identifier(name) = &argument.kind else {
                    return None;
                };
                parameters.get(name.as_str()).copied()
            })
            .collect::<Option<Vec<_>>>()?
            .into_boxed_slice();
        Some(Self {
            name: JsString::from(name.as_str()),
            arguments,
        })
    }

    pub(super) fn execute(
        &self,
        closure: &StoredFunctionClosure,
        owner: &Object,
        this: Value,
        arguments: &[Value],
        realm: &Realm,
    ) -> RuntimeResult<Value> {
        let _roots = realm.enter_object_root_region();
        let (receiver, function) = if closure.kind == FunctionKind::Arrow {
            let mut current = Some(match &closure.lexical_this_environment {
                Some(environment) => environment.load(owner)?,
                None => closure.environment.load(owner)?,
            });
            loop {
                let environment = current.ok_or_else(|| {
                    RuntimeError::reference_error("super property has no function environment")
                })?;
                if let Some(binding) = environment.super_reference_binding()? {
                    break binding;
                }
                current = environment.outer();
            }
        } else {
            let receiver = if closure.code.strict {
                this
            } else {
                ordinary_this(realm, this)?
            };
            (receiver, owner.clone())
        };
        // Read the current parent on every call; getter/Proxy effects and the
        // original receiver still pass through the shared Reference machinery.
        let reference = Reference::property(
            function.super_base()?,
            ReferencedName::PropertyKey(PropertyKey::String(self.name.clone())),
            closure.code.strict,
            Some(receiver.clone()),
        );
        let target = reference.get_value()?;
        let forwarded = self
            .arguments
            .iter()
            .map(|&index| arguments.get(index).cloned().unwrap_or(Value::Undefined))
            .collect::<Vec<_>>();
        let Value::Object(target) = target else {
            return Err(RuntimeError::type_error("value is not callable"));
        };
        if !target.is_callable() {
            return Err(RuntimeError::type_error("value is not callable"));
        }
        // Eligibility was decided before executing effects. Never retry errors
        // through the AST evaluator, including errors thrown by the target.
        target.call(receiver, &forwarded)
    }
}
