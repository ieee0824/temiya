use super::*;

#[derive(Debug)]
enum Receiver {
    This,
    Argument(usize),
}
#[derive(Debug)]
enum Operation {
    Read,
    Update { increment: bool, prefix: bool },
    Has,
}

// Shared code caches syntax and parameter positions only. A closure may bind
// its immutable private identity once, after class names have been declared.
#[derive(Debug)]
pub(super) struct Plan {
    name: JsString,
    receiver: Receiver,
    operation: Operation,
}

pub(super) fn prepare(function: &Function, kind: FunctionKind) -> Option<Plan> {
    if !(kind == FunctionKind::Arrow
        || kind == FunctionKind::Method(MethodKind::Method) && function.strict)
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
    let expression = match &function.body {
        FunctionBody::Expression(expression) if kind == FunctionKind::Arrow => expression.as_ref(),
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
        _ => return None,
    };
    let (target, operation) = match &expression.kind {
        ExprKind::Binary {
            operator,
            left,
            right,
        } if operator == "in" => {
            let ExprKind::PrivateIdentifier(name) = &left.kind else {
                return None;
            };
            return Some(Plan {
                name: name.as_str().into(),
                receiver: receiver(right, &parameters)?,
                operation: Operation::Has,
            });
        }
        ExprKind::Update {
            operator,
            argument,
            prefix,
        } if operator == "++" || operator == "--" => (
            argument.as_ref(),
            Operation::Update {
                increment: operator == "++",
                prefix: *prefix,
            },
        ),
        _ => (expression, Operation::Read),
    };
    let ExprKind::Member {
        object,
        property,
        computed: false,
        optional: false,
    } = &target.kind
    else {
        return None;
    };
    let ExprKind::PrivateIdentifier(name) = &property.kind else {
        return None;
    };
    Some(Plan {
        name: name.as_str().into(),
        receiver: receiver(object, &parameters)?,
        operation,
    })
}

fn receiver(expression: &Expr, parameters: &HashMap<&str, usize>) -> Option<Receiver> {
    match &expression.kind {
        ExprKind::This => Some(Receiver::This),
        ExprKind::Identifier(name) => parameters
            .get(name.as_str())
            .copied()
            .map(Receiver::Argument),
        _ => None,
    }
}

impl Plan {
    pub(super) fn bind(
        &self,
        environment: &Environment,
    ) -> Option<super::super::private_fields::Name> {
        // A missing name must still fail at invocation, preserving error order.
        environment.resolve_private_key(&self.name).ok()
    }

    pub(super) fn execute(
        &self,
        closure: &StoredFunctionClosure,
        owner: &Object,
        this: Value,
        arguments: &[Value],
        realm: &Realm,
    ) -> RuntimeResult<Value> {
        let receiver = match self.receiver {
            Receiver::Argument(index) => arguments.get(index).cloned().unwrap_or(Value::Undefined),
            Receiver::This if closure.kind == FunctionKind::Arrow => {
                match &closure.lexical_this_environment {
                    Some(environment) => environment.load(owner)?.get_this_binding()?,
                    None => closure.lexical_this.load(owner)?,
                }
            }
            Receiver::This => this,
        };
        // Private `in` checks the RHS type before resolving the private name.
        if matches!(self.operation, Operation::Has) && !matches!(receiver, Value::Object(_)) {
            return Err(RuntimeError::type_error(
                "private in receiver is not an object",
            ));
        }
        let name = match &closure.private_expression_name {
            Some(name) => name.clone(),
            None => closure
                .environment
                .load(owner)?
                .resolve_private_key(&self.name)?,
        };
        match self.operation {
            Operation::Has => {
                let Value::Object(object) = receiver else {
                    unreachable!()
                };
                Ok(Value::Boolean(object.has_private(&name)))
            }
            Operation::Read => super::super::Reference::private(receiver, name).get_value(),
            Operation::Update { increment, prefix } => {
                super::super::interpreter::update_reference_in_realm(
                    realm,
                    &super::super::Reference::private(receiver, name),
                    increment,
                    prefix,
                )
            }
        }
    }
}
