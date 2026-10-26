//! Synchronous iterator records used by argument-list evaluation.

use super::{
    IntrinsicId, IntrinsicRegistry, JsString, Object, PropertyDescriptor, PropertyKey, Realm,
    Reference, ReferencedName, RuntimeError, RuntimeResult, Value, WellKnownSymbol,
    WellKnownSymbols, array_methods::method, coercion::primitive,
};

pub(crate) const MAX_ARGUMENTS: usize = 65_536;

pub(super) enum IteratorSlot {
    Array {
        target: Option<Object>,
        index: u64,
    },
    String {
        text: Option<JsString>,
        index: usize,
    },
}

pub(crate) fn push_argument(arguments: &mut Vec<Value>, value: Value) -> RuntimeResult<()> {
    if arguments.len() >= MAX_ARGUMENTS {
        return Err(RuntimeError::range_error("maximum argument count exceeded"));
    }
    arguments.push(value);
    Ok(())
}

pub(crate) fn spread_arguments(
    realm: &Realm,
    value: Value,
    arguments: &mut Vec<Value>,
) -> RuntimeResult<()> {
    let method = Reference::property_in_realm(
        realm,
        value.clone(),
        ReferencedName::PropertyKey(realm.well_known_symbol(WellKnownSymbol::Iterator).into()),
        false,
    )
    .get_value()?;
    let Value::Object(method) = method else {
        return Err(RuntimeError::type_error("spread value is not iterable"));
    };
    let Value::Object(iterator) = method.call(value, &[])? else {
        return Err(RuntimeError::type_error(
            "iterator method returned a non-object",
        ));
    };
    // Capture next once; replacing iterator.next during iteration does not change
    // this Iterator Record. No object/slot borrow survives any user callback.
    let next = iterator.get(&"next".into())?;
    loop {
        let Value::Object(next) = &next else {
            return Err(RuntimeError::type_error("iterator next is not callable"));
        };
        let Value::Object(result) = next.call(Value::Object(iterator.clone()), &[])? else {
            return Err(RuntimeError::type_error(
                "iterator next returned a non-object",
            ));
        };
        if result.get(&"done".into())?.to_boolean() {
            return Ok(());
        }
        let value = result.get(&"value".into())?;
        if let Err(error) = push_argument(arguments, value) {
            // An implementation argument limit interrupts consumption. Close the
            // iterator with a throw completion; that completion takes priority
            // over return getter/call failures (IteratorClose steps 3–5).
            if let Ok(Value::Object(return_method)) = iterator.get(&"return".into()) {
                let _ = return_method.call(Value::Object(iterator.clone()), &[]);
            }
            return Err(error);
        }
    }
}

pub(super) fn install(
    intrinsics: &IntrinsicRegistry,
    symbols: &WellKnownSymbols,
) -> RuntimeResult<()> {
    let function = intrinsics.get(IntrinsicId::FunctionPrototype);
    let object = intrinsics.get(IntrinsicId::ObjectPrototype);
    let iterator = intrinsics.get(IntrinsicId::IteratorPrototype);
    let array_iterator = intrinsics.get(IntrinsicId::ArrayIteratorPrototype);
    let string_iterator = intrinsics.get(IntrinsicId::StringIteratorPrototype);
    let iterator_key: PropertyKey = symbols.get(WellKnownSymbol::Iterator).into();
    iterator.set_prototype(Some(object.clone()));
    array_iterator.set_prototype(Some(iterator.clone()));
    string_iterator.set_prototype(Some(iterator.clone()));
    symbol_method(
        &iterator,
        &function,
        iterator_key.clone(),
        "[Symbol.iterator]",
        |this, _| Ok(this),
    )?;
    for (prototype, tag) in [
        (&array_iterator, "Array Iterator"),
        (&string_iterator, "String Iterator"),
    ] {
        prototype.define_own_property(
            symbols.get(WellKnownSymbol::ToStringTag).into(),
            PropertyDescriptor::data(Value::String(tag.into()), false, false, true),
        )?;
    }
    let result_prototype = object.clone();
    let to_primitive: PropertyKey = symbols.get(WellKnownSymbol::ToPrimitive).into();
    let length_key = to_primitive.clone();
    method(&array_iterator, &function, "next", 0, move |this, _| {
        array_next(this, &length_key, &result_prototype)
    })?;
    let result_prototype = object;
    method(&string_iterator, &function, "next", 0, move |this, _| {
        string_next(this, &result_prototype)
    })?;
    let array = intrinsics.get(IntrinsicId::ArrayPrototype);
    // Borrow the same ToObject implementation as other generic Array methods.
    let boxing = super::array_methods::iterator_boxing(intrinsics, symbols);
    method(&array, &function, "values", 0, move |this, _| {
        let target = boxing(this)?;
        let result = Object::new();
        result.set_prototype(Some(array_iterator.clone()));
        result.0.state.borrow_mut().iterator = Some(Box::new(IteratorSlot::Array {
            target: Some(target),
            index: 0,
        }));
        Ok(Value::Object(result))
    })?;
    array.define_own_property(
        iterator_key.clone(),
        PropertyDescriptor::data(array.get(&"values".into())?, true, false, true),
    )?;
    symbol_method(
        &intrinsics.get(IntrinsicId::StringPrototype),
        &function,
        iterator_key,
        "[Symbol.iterator]",
        move |this, _| {
            if matches!(this, Value::Undefined | Value::Null) {
                return Err(RuntimeError::type_error(
                    "string iterator receiver is null or undefined",
                ));
            }
            let text = primitive(&this, &to_primitive, true)?.to_js_string()?;
            let result = Object::new();
            result.set_prototype(Some(string_iterator.clone()));
            result.0.state.borrow_mut().iterator = Some(Box::new(IteratorSlot::String {
                text: Some(text),
                index: 0,
            }));
            Ok(Value::Object(result))
        },
    )?;
    Ok(())
}

fn symbol_method(
    holder: &Object,
    prototype: &Object,
    key: PropertyKey,
    name: &str,
    callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
) -> RuntimeResult<()> {
    let function = Object::native_function(callback);
    function.set_prototype(Some(prototype.clone()));
    function.define_own_property(
        "name".into(),
        PropertyDescriptor::data(Value::String(name.into()), false, false, true),
    )?;
    function.define_own_property(
        "length".into(),
        PropertyDescriptor::data(Value::Number(0.0), false, false, true),
    )?;
    holder.define_own_property(
        key,
        PropertyDescriptor::data(Value::Object(function), true, false, true),
    )?;
    Ok(())
}

fn result(prototype: &Object, value: Value, done: bool) -> RuntimeResult<Value> {
    let result = Object::new();
    result.set_prototype(Some(prototype.clone()));
    result.create_data_property("value".into(), value)?;
    result.create_data_property("done".into(), Value::Boolean(done))?;
    Ok(Value::Object(result))
}

fn array_next(this: Value, to_primitive: &PropertyKey, prototype: &Object) -> RuntimeResult<Value> {
    let Value::Object(iterator) = this else {
        return Err(RuntimeError::type_error(
            "incompatible array iterator receiver",
        ));
    };
    let (target, index) = {
        let state = iterator.0.state.borrow();
        let Some(IteratorSlot::Array { target, index }) = state.iterator.as_deref() else {
            return Err(RuntimeError::type_error(
                "incompatible array iterator receiver",
            ));
        };
        (target.clone(), *index)
    };
    let Some(target) = target else {
        return result(prototype, Value::Undefined, true);
    };
    let length = primitive(&target.get(&"length".into())?, to_primitive, false)?.to_number()?;
    let length = (length as u64).min(9_007_199_254_740_991);
    if index >= length {
        if let Some(IteratorSlot::Array { target, .. }) =
            iterator.0.state.borrow_mut().iterator.as_deref_mut()
        {
            *target = None;
        }
        return result(prototype, Value::Undefined, true);
    }
    // The captured index is advanced after length coercion, before element Get,
    // matching reentrant getters and abrupt element access without RefCell borrows.
    if let Some(IteratorSlot::Array { index: next, .. }) =
        iterator.0.state.borrow_mut().iterator.as_deref_mut()
    {
        *next = index + 1;
    }
    let value = target.get(&JsString::from(index.to_string()).into())?;
    result(prototype, value, false)
}

fn string_next(this: Value, prototype: &Object) -> RuntimeResult<Value> {
    let Value::Object(iterator) = this else {
        return Err(RuntimeError::type_error(
            "incompatible string iterator receiver",
        ));
    };
    let value = {
        let mut state = iterator.0.state.borrow_mut();
        let Some(IteratorSlot::String { text, index }) = state.iterator.as_deref_mut() else {
            return Err(RuntimeError::type_error(
                "incompatible string iterator receiver",
            ));
        };
        if let Some(string) = text {
            let units = string.code_units();
            if *index >= units.len() {
                *text = None;
                None
            } else {
                let width = if (0xD800..=0xDBFF).contains(&units[*index])
                    && units
                        .get(*index + 1)
                        .is_some_and(|unit| (0xDC00..=0xDFFF).contains(unit))
                {
                    2
                } else {
                    1
                };
                let value = JsString::from_code_units_slice(&units[*index..*index + width]);
                *index += width;
                Some(Value::String(value))
            }
        } else {
            None
        }
    };
    let done = value.is_none();
    result(prototype, value.unwrap_or(Value::Undefined), done)
}
