//! Array binding initialization shared by declarations and formal parameters.
use super::{
    Completion, ExpressionEvaluator, JsString, Reference, RuntimeError, RuntimeResult, Value,
    iterator::IteratorRecord,
};
use crate::ast::{Expr, ExprKind};

pub(super) fn names(binding: &Expr, output: &mut Vec<JsString>) -> RuntimeResult<()> {
    match &binding.kind {
        ExprKind::Identifier(name) => output.push(name.as_str().into()),
        ExprKind::Assignment { target, .. } | ExprKind::Spread(target) => names(target, output)?,
        ExprKind::Array { elements, .. } => {
            for element in elements.iter().flatten() {
                names(element, output)?;
            }
        }
        _ => {
            return Err(RuntimeError::internal_error(
                "object binding execution is not implemented",
            ));
        }
    }
    Ok(())
}

// Identifier declarations must not allocate a temporary list just because
// patterns are supported. Patterns still collect completely before callbacks,
// retaining the previous failure order for unsupported nested syntax.
#[inline]
pub(super) fn for_each_name(
    binding: &Expr,
    mut visit: impl FnMut(JsString) -> RuntimeResult<()>,
) -> RuntimeResult<()> {
    if let ExprKind::Identifier(name) = &binding.kind {
        return visit(name.as_str().into());
    }
    let mut bound_names = Vec::new();
    names(binding, &mut bound_names)?;
    for name in bound_names {
        visit(name)?;
    }
    Ok(())
}

enum Target {
    Initialize(JsString),
    Assign(Reference),
}
impl Target {
    fn prepare(
        evaluator: &ExpressionEvaluator,
        binding: &Expr,
        initialize: bool,
    ) -> RuntimeResult<Option<Self>> {
        let ExprKind::Identifier(name) = &binding.kind else {
            return Ok(None);
        };
        Ok(Some(if initialize {
            Self::Initialize(name.as_str().into())
        } else {
            Self::Assign(evaluator.evaluate_reference(binding)?)
        }))
    }
    fn put(self, evaluator: &ExpressionEvaluator, value: Value) -> RuntimeResult<()> {
        match self {
            Self::Initialize(name) => evaluator.environment.initialize_binding(&name, value),
            Self::Assign(reference) => reference.put_value(value),
        }
    }
}

pub(super) fn initialize(
    evaluator: &ExpressionEvaluator,
    binding: &Expr,
    value: Value,
    initialize: bool,
) -> RuntimeResult<()> {
    element(evaluator, binding, &mut || Ok(value.clone()), initialize)
}

fn element(
    evaluator: &ExpressionEvaluator,
    binding: &Expr,
    get_value: &mut dyn FnMut() -> RuntimeResult<Value>,
    initialize: bool,
) -> RuntimeResult<()> {
    let (binding, default) = match &binding.kind {
        ExprKind::Assignment { target, value, .. } => (target.as_ref(), Some(value.as_ref())),
        _ => (binding, None),
    };
    // Resolving a var binding can invoke an object environment's HasBinding.
    // It precedes IteratorStepValue and default evaluation.
    let target = Target::prepare(evaluator, binding, initialize)?;
    let mut value = get_value()?;
    if matches!(value, Value::Undefined) {
        if let Some(default) = default {
            value = if let ExprKind::Identifier(name) = &binding.kind {
                evaluator.evaluate_named(default, &name.as_str().into())?
            } else {
                evaluator.evaluate(default)?
            };
        }
    }
    if let Some(target) = target {
        return target.put(evaluator, value);
    }
    let ExprKind::Array { elements, .. } = &binding.kind else {
        return Err(RuntimeError::internal_error(
            "object binding execution is not implemented",
        ));
    };
    let mut iterator = IteratorRecord::get(&evaluator.realm, value)?;
    let result = (|| {
        for entry in elements {
            match entry {
                None => {
                    if !iterator.is_done() {
                        iterator.step()?;
                    }
                }
                Some(Expr {
                    kind: ExprKind::Spread(rest),
                    ..
                }) => {
                    let target = Target::prepare(evaluator, rest, initialize)?;
                    let array = super::array_literal::create(&evaluator.realm)?;
                    let mut next = 0;
                    while !iterator.is_done() {
                        let Some(value) = iterator.step_value()? else {
                            break;
                        };
                        super::array_literal::append(&array, &mut next, value)?;
                    }
                    super::array_literal::finish(&array, next)?;
                    let value = Value::Object(array);
                    if let Some(target) = target {
                        target.put(evaluator, value)?;
                    } else {
                        self::initialize(evaluator, rest, value, initialize)?;
                    }
                }
                Some(binding) => element(
                    evaluator,
                    binding,
                    &mut || {
                        if iterator.is_done() {
                            Ok(Value::Undefined)
                        } else {
                            Ok(iterator.step_value()?.unwrap_or(Value::Undefined))
                        }
                    },
                    initialize,
                )?,
            }
        }
        Ok(Completion::Normal(None))
    })();
    if iterator.is_done() {
        result.map(|_| ())
    } else {
        iterator.close(result).map(|_| ())
    }
}
