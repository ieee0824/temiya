//! Shared object-initializer operations; evaluation order belongs to the caller.
use super::{
    IntrinsicId, JsString, Object, PrimitiveHint, PropertyKey, Realm, RuntimeError, RuntimeResult,
    Value, WellKnownSymbol,
};

pub(crate) fn create(realm: &Realm) -> Object {
    Object::new_with_prototype(realm.intrinsic(IntrinsicId::ObjectPrototype))
}

pub(crate) fn property_key(realm: &Realm, value: Value) -> RuntimeResult<PropertyKey> {
    if !matches!(value, Value::Object(_)) {
        return value.to_property_key();
    }
    super::coercion::primitive_with_hint(
        &value,
        &realm.well_known_symbol(WellKnownSymbol::ToPrimitive).into(),
        PrimitiveHint::String,
    )?
    .to_property_key()
}

pub(crate) fn define_data(
    object: &Object,
    key: PropertyKey,
    value: Value,
    prototype_setter: bool,
) -> RuntimeResult<()> {
    let accepted = if prototype_setter {
        match value {
            Value::Object(prototype) => object.try_set_prototype(Some(prototype))?,
            Value::Null => object.try_set_prototype(None)?,
            _ => true,
        }
    } else {
        object.create_data_property(key, value)?
    };
    if accepted {
        Ok(())
    } else {
        Err(RuntimeError::type_error(
            "object initializer property was rejected",
        ))
    }
}

pub(crate) fn spread(object: &Object, value: Value) -> RuntimeResult<()> {
    spread_excluding(object, value, &[])
}

pub(super) fn spread_excluding(
    object: &Object,
    value: Value,
    excluded: &[PropertyKey],
) -> RuntimeResult<()> {
    let source = match value {
        Value::Object(source) => source,
        Value::String(string) => {
            // Only the enumerable UTF-16 indices of the boxed String are copied.
            for (index, unit) in string.code_units().iter().enumerate() {
                let key = PropertyKey::String(index.to_string().into());
                if excluded.contains(&key) {
                    continue;
                }
                define_data(
                    object,
                    key,
                    Value::String(JsString::from_code_units_slice(&[*unit])),
                    false,
                )?;
            }
            return Ok(());
        }
        _ => return Ok(()),
    };
    for key in source.try_own_property_keys()? {
        if excluded.contains(&key) {
            continue;
        }
        let value = match source.try_own_enumerable_value(&key) {
            Some(value) => value?,
            None => {
                if source
                    .try_get_own_property(&key)?
                    .is_some_and(|descriptor| descriptor.enumerable == Some(true))
                {
                    Some(source.get(&key)?)
                } else {
                    None
                }
            }
        };
        if let Some(value) = value {
            define_data(object, key, value, false)?;
        }
    }
    Ok(())
}
