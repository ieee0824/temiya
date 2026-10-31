use super::*;

// Strict zero-parameter methods with only a private read/update cannot expose
// a function activation. Evaluate the unchanged expression with its captured
// class environment; private identity resolution, coercion and writes still use
// the shared evaluator. Code caches never contain class-specific private names.
pub(super) fn prepare(function: &Function, kind: FunctionKind) -> Option<Rc<Expr>> {
    if kind != FunctionKind::Method(MethodKind::Method)
        || !function.strict
        || function.asynchronous
        || function.generator
        || !function.parameters.is_empty()
    {
        return None;
    }
    let FunctionBody::Block(body) = &function.body else {
        return None;
    };
    let StmtKind::Block(statements) = &body.kind else {
        return None;
    };
    let [statement] = statements.as_slice() else {
        return None;
    };
    let StmtKind::Return(Some(expression)) = &statement.kind else {
        return None;
    };
    let member = match &expression.kind {
        ExprKind::Update {
            operator, argument, ..
        } if operator == "++" || operator == "--" => argument.as_ref(),
        _ => expression,
    };
    let ExprKind::Member {
        object,
        property,
        computed: false,
        optional: false,
    } = &member.kind
    else {
        return None;
    };
    if !matches!(object.kind, ExprKind::This)
        || !matches!(property.kind, ExprKind::PrivateIdentifier(_))
    {
        return None;
    }
    Some(Rc::new(expression.clone()))
}
