//! Binding initialization shared by declarations and formal parameters.
use super::{
    Completion, ExpressionEvaluator, JsString, PropertyKey, Reference, ReferencedName,
    RuntimeError, RuntimeResult, Value, iterator::IteratorRecord,
};
use crate::ast::{Expr, ExprKind, Literal, ObjectProperty};

pub(super) fn names(binding: &Expr, output: &mut Vec<JsString>) -> RuntimeResult<()> {
    match &binding.kind {
        ExprKind::Identifier(name) => output.push(name.as_str().into()),
        ExprKind::Assignment { target, .. } | ExprKind::Spread(target) => names(target, output)?,
        ExprKind::Array { elements, .. } => {
            for element in elements.iter().flatten() {
                names(element, output)?;
            }
        }
        ExprKind::Object(properties) => {
            for property in properties {
                names(property.value.as_ref().unwrap_or(&property.key), output)?;
            }
        }
        _ => {
            return Err(RuntimeError::internal_error("invalid binding pattern"));
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
    if let ExprKind::Object(properties) = &binding.kind {
        return object(evaluator, properties, value, initialize);
    }
    let ExprKind::Array { elements, .. } = &binding.kind else {
        return Err(RuntimeError::internal_error("invalid binding pattern"));
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

fn object(
    evaluator: &ExpressionEvaluator,
    properties: &[ObjectProperty],
    value: Value,
    initialize: bool,
) -> RuntimeResult<()> {
    // RequireObjectCoercible precedes even computed-key evaluation and empty
    // patterns. GetV below retains the original primitive as the receiver.
    if matches!(value, Value::Undefined | Value::Null) {
        return Err(RuntimeError::type_error(
            "cannot destructure null or undefined",
        ));
    }
    let has_rest = properties.iter().any(|property| property.spread);
    let mut excluded = if has_rest {
        Vec::with_capacity(properties.len())
    } else {
        Vec::new()
    };
    for property in properties {
        if property.spread {
            let target = Target::prepare(evaluator, &property.key, initialize)?;
            let rest = super::object_literal::create(&evaluator.realm);
            super::object_literal::spread_excluding(&rest, value.clone(), &excluded)?;
            let target = target
                .ok_or_else(|| RuntimeError::internal_error("invalid object rest binding"))?;
            target.put(evaluator, Value::Object(rest))?;
            continue;
        }
        let key = if property.computed
            || matches!(property.key.kind, ExprKind::Literal(Literal::Number { .. }))
        {
            super::object_literal::property_key(
                &evaluator.realm,
                evaluator.evaluate(&property.key)?,
            )?
        } else {
            PropertyKey::String(super::interpreter::member_name(&property.key)?)
        };
        let binding = property.value.as_ref().unwrap_or(&property.key);
        element(
            evaluator,
            binding,
            &mut || match &value {
                Value::Object(object) => object.get_with_receiver(&key, value.clone()),
                _ => Reference::property_in_realm(
                    &evaluator.realm,
                    value.clone(),
                    ReferencedName::PropertyKey(key.clone()),
                    evaluator.strict,
                )
                .get_value(),
            },
            initialize,
        )?;
        if has_rest {
            excluded.push(key);
        }
    }
    Ok(())
}
