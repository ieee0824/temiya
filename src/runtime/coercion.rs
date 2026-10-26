//! Realm-aware JavaScript coercion for built-in algorithms.
use super::{PrimitiveHint, PropertyKey, RuntimeError, RuntimeResult, Value};

pub(super) fn primitive(
    value: &Value,
    to_primitive: &PropertyKey,
    string_hint: bool,
) -> RuntimeResult<Value> {
    primitive_with_hint(
        value,
        to_primitive,
        if string_hint {
            PrimitiveHint::String
        } else {
            PrimitiveHint::Number
        },
    )
}

pub(super) fn primitive_with_hint(
    value: &Value,
    to_primitive: &PropertyKey,
    hint: PrimitiveHint,
) -> RuntimeResult<Value> {
    let Value::Object(object) = &value else {
        return Ok(value.clone());
    };
    let hooks = object.0.conversion.borrow();
    let has_host_conversion =
        hooks.exotic.is_some() || hooks.value_of.is_some() || hooks.to_string.is_some();
    drop(hooks);
    if has_host_conversion {
        return value.to_primitive(hint);
    }
    // Unlike bootstrap Rust conversion hooks, this path must call actual
    // JavaScript methods, including getters and Proxy traps.
    let exotic = object.get(to_primitive)?;
    if !matches!(exotic, Value::Undefined | Value::Null) {
        let Value::Object(method) = exotic else {
            return Err(RuntimeError::type_error("@@toPrimitive is not callable"));
        };
        let result = method.call(
            value.clone(),
            &[Value::String(
                match hint {
                    PrimitiveHint::Default => "default",
                    PrimitiveHint::String => "string",
                    PrimitiveHint::Number => "number",
                }
                .into(),
            )],
        )?;
        if matches!(result, Value::Object(_)) {
            return Err(RuntimeError::type_error("@@toPrimitive returned an object"));
        }
        return Ok(result);
    }
    for name in if hint == PrimitiveHint::String {
        ["toString", "valueOf"]
    } else {
        ["valueOf", "toString"]
    } {
        if let Value::Object(method) = object.get(&name.into())? {
            if method.is_callable() {
                let result = method.call(value.clone(), &[])?;
                if !matches!(result, Value::Object(_)) {
                    return Ok(result);
                }
            }
        }
    }
    Err(RuntimeError::type_error(
        "cannot convert object to a primitive",
    ))
}
