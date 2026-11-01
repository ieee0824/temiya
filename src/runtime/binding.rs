//! Binding initialization shared by declarations and formal parameters.
use super::{
    Completion, ExpressionEvaluator, JsString, PropertyKey, Reference, ReferencedName,
    RuntimeError, RuntimeResult, Value, iterator::IteratorRecord,
};
use crate::ast::{Expr, ExprKind, Literal, ObjectProperty};

const NULLISH_OBJECT_MESSAGE: &str = "cannot destructure null or undefined";
thread_local! {
    // One fixed Rust string per thread, without a Realm, Object or GC root.
    static NULLISH_OBJECT_DIAGNOSTIC: std::rc::Rc<String> =
        std::rc::Rc::new(NULLISH_OBJECT_MESSAGE.to_owned());
}
pub(super) fn nullish_object_error() -> RuntimeError {
    NULLISH_OBJECT_DIAGNOSTIC
        .try_with(|message| RuntimeError::type_error_shared(message.clone()))
        .unwrap_or_else(|_| RuntimeError::type_error(NULLISH_OBJECT_MESSAGE))
}

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

// Validate the complete pattern before invoking callbacks, preserving the
// failure order without allocating an intermediate list of bound names.
fn visit_names(
    binding: &Expr,
    visit: &mut impl FnMut(&str) -> RuntimeResult<()>,
) -> RuntimeResult<()> {
    match &binding.kind {
        ExprKind::Identifier(name) => visit(name.as_str()),
        ExprKind::Assignment { target, .. } | ExprKind::Spread(target) => {
            visit_names(target, visit)
        }
        ExprKind::Array { elements, .. } => {
            for element in elements.iter().flatten() {
                visit_names(element, visit)?;
            }
            Ok(())
        }
        ExprKind::Object(properties) => {
            for property in properties {
                visit_names(property.value.as_ref().unwrap_or(&property.key), visit)?;
            }
            Ok(())
        }
        _ => Err(RuntimeError::internal_error("invalid binding pattern")),
    }
}

#[inline]
pub(super) fn for_each_name(
    binding: &Expr,
    mut visit: impl FnMut(JsString) -> RuntimeResult<()>,
) -> RuntimeResult<()> {
    if let ExprKind::Identifier(name) = &binding.kind {
        return visit(name.as_str().into());
    }
    visit_names(binding, &mut |_| Ok(()))?;
    visit_names(binding, &mut |name| visit(name.into()))
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
            if initialize
                && matches!(binding.kind, ExprKind::Object(_))
                && matches!(default.kind, ExprKind::Object(_))
                && initialize_literal_default(evaluator, binding, default)?
            {
                return Ok(());
            }
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

// Values remain rooted Rust Values until initialization completes. The one-key
// case needs no buffer allocation; overwrites release the previous value at
// the same point as an ordinary own data property overwrite.
enum LiteralValues<'a> {
    Empty,
    One(LiteralKey<'a>, Value),
    Many(Vec<(LiteralKey<'a>, Value)>),
}
impl<'a> LiteralValues<'a> {
    fn insert(&mut self, key: LiteralKey<'a>, value: Value) {
        match self {
            Self::Empty => *self = Self::One(key, value),
            Self::One(old, entry) if *old == key => *entry = value,
            Self::One(_, _) => {
                let Self::One(old, entry) = std::mem::replace(self, Self::Empty) else {
                    unreachable!()
                };
                *self = Self::Many(vec![(old, entry), (key, value)]);
            }
            Self::Many(entries) => {
                if let Some((_, entry)) = entries.iter_mut().find(|(old, _)| *old == key) {
                    *entry = value;
                } else {
                    entries.push((key, value));
                }
            }
        }
    }
    fn get(&self, key: &LiteralKey<'_>) -> Option<&Value> {
        match self {
            Self::Empty => None,
            Self::One(old, value) => (*old == *key).then_some(value),
            Self::Many(entries) => entries
                .iter()
                .find(|(old, _)| old == key)
                .map(|(_, value)| value),
        }
    }
}
// Static keys borrow immutable syntax; comparisons preserve exact UTF-16
// equality without repeatedly converting identifier text into runtime strings.
#[derive(Clone, Copy)]
enum LiteralKey<'a> {
    Identifier(&'a str),
    String(&'a [u16]),
}
impl PartialEq for LiteralKey<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Identifier(left), Self::Identifier(right)) => left == right,
            (Self::String(left), Self::String(right)) => left == right,
            (Self::Identifier(text), Self::String(units))
            | (Self::String(units), Self::Identifier(text)) => {
                text.encode_utf16().eq(units.iter().copied())
            }
        }
    }
}
fn literal_key(expression: &Expr) -> Option<LiteralKey<'_>> {
    match &expression.kind {
        ExprKind::Identifier(name) => Some(LiteralKey::Identifier(name.as_str())),
        ExprKind::Literal(Literal::String { value, .. }) => Some(LiteralKey::String(value)),
        _ => None,
    }
}

// A fresh literal containing only own data properties cannot be observed by a
// static object pattern when every requested key is present. Preserve all value
// evaluation and heap-context validation before initializing any target. Other
// shapes keep the ordinary object/receiver/prototype path.
#[inline(never)]
fn initialize_literal_default(
    evaluator: &ExpressionEvaluator,
    binding: &Expr,
    default: &Expr,
) -> RuntimeResult<bool> {
    let (ExprKind::Object(pattern), ExprKind::Object(properties)) = (&binding.kind, &default.kind)
    else {
        return Ok(false);
    };
    if properties.iter().any(|property| {
        property.spread
            || property.computed
            || literal_key(&property.key)
                .is_none_or(|key| key == LiteralKey::Identifier("__proto__"))
            || matches!(
                property.value.as_ref().map(|value| &value.kind),
                Some(ExprKind::Method { .. })
            )
    }) {
        return Ok(false);
    }
    for property in pattern {
        if property.spread || property.computed {
            return Ok(false);
        }
        let Some(key) = literal_key(&property.key) else {
            return Ok(false);
        };
        if !properties
            .iter()
            .any(|property| literal_key(&property.key).as_ref() == Some(&key))
        {
            return Ok(false);
        }
    }
    let mut values = LiteralValues::Empty;
    for property in properties {
        let key = literal_key(&property.key).expect("validated literal key");
        let value = evaluator.evaluate(property.value.as_ref().unwrap_or(&property.key))?;
        if let Value::Object(object) = &value {
            super::stored::StoredObject::check_context(&evaluator.realm, object)?;
        }
        values.insert(key, value);
    }
    for property in pattern {
        let key = literal_key(&property.key).expect("validated pattern key");
        let value = values.get(&key).expect("present own literal data").clone();
        element(
            evaluator,
            property.value.as_ref().unwrap_or(&property.key),
            &mut || Ok(value.clone()),
            true,
        )?;
    }
    Ok(true)
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
        return Err(nullish_object_error());
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
            let fast = if let Value::Object(source) = &value {
                source.copy_ordinary_data_rest(
                    &evaluator
                        .realm
                        .intrinsic(super::IntrinsicId::ObjectPrototype),
                    &excluded,
                )
            } else {
                None
            };
            let rest = match fast {
                Some(rest) => rest,
                None => {
                    let rest = super::object_literal::create(&evaluator.realm);
                    super::object_literal::spread_excluding(&rest, value.clone(), &excluded)?;
                    rest
                }
            };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_nested_pattern_is_rejected_before_binding_callbacks() {
        let expr = |kind| Expr {
            span: crate::lexer::Span { start: 0, end: 0 },
            kind,
        };
        let pattern = expr(ExprKind::Array {
            elements: vec![
                Some(expr(ExprKind::Identifier("first".into()))),
                Some(expr(ExprKind::Array {
                    elements: vec![Some(expr(ExprKind::This))],
                    trailing_comma: false,
                })),
            ],
            trailing_comma: false,
        });
        let mut callbacks = 0;
        assert!(
            for_each_name(&pattern, |_| {
                callbacks += 1;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(callbacks, 0);
    }
}
