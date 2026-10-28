//! Synchronous iterator records used by argument-list evaluation.

use super::{
    IntrinsicId, IntrinsicRegistry, JsString, Object, PropertyDescriptor, PropertyKey, Realm,
    Reference, ReferencedName, RuntimeError, RuntimeResult, Value, WellKnownSymbol,
    WellKnownSymbols, coercion::primitive,
};

pub(crate) const MAX_ARGUMENTS: usize = 65_536;

pub(super) enum IteratorSlot {
    Array {
        target: Option<super::stored::StoredObject>,
        index: u64,
    },
    String {
        text: Option<JsString>,
        index: usize,
    },
}

impl IteratorSlot {
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        if let Self::Array {
            target: Some(target),
            ..
        } = self
        {
            target.trace(tracer);
        }
    }
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
    consume_iterable(realm, value, |value| push_argument(arguments, value))
}

/// Consume a synchronous iterator without materializing an intermediate list.
pub(crate) fn consume_iterable(
    realm: &Realm,
    value: Value,
    mut append: impl FnMut(Value) -> RuntimeResult<()>,
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
        if let Err(error) = append(value) {
            // A consumer error (including the argument limit) interrupts consumption. Close the
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
    let to_primitive: PropertyKey = symbols.get(WellKnownSymbol::ToPrimitive).into();
    let length_key = to_primitive.clone();
    let next = Object::native_function_with_object_capture(
        &function,
        "next",
        0,
        &object,
        Some(length_key),
        |prototype, key, this, _| array_next(this, key.expect("iterator primitive key"), prototype),
    )?;
    array_iterator.define_own_property(
        "next".into(),
        PropertyDescriptor::data(Value::Object(next), true, false, true),
    )?;
    let next = Object::native_function_with_object_capture(
        &function,
        "next",
        0,
        &object,
        None,
        |prototype, _, this, _| string_next(this, prototype),
    )?;
    string_iterator.define_own_property(
        "next".into(),
        PropertyDescriptor::data(Value::Object(next), true, false, true),
    )?;
    let array = intrinsics.get(IntrinsicId::ArrayPrototype);
    // Borrow the same ToObject implementation as other generic Array methods.
    let values = super::array_methods::values_function(intrinsics, symbols, &array_iterator)?;
    array.define_own_property(
        "values".into(),
        PropertyDescriptor::data(Value::Object(values), true, false, true),
    )?;
    array.define_own_property(
        iterator_key.clone(),
        PropertyDescriptor::data(array.get(&"values".into())?, true, false, true),
    )?;
    let values = Object::native_function_with_object_capture(
        &function,
        "[Symbol.iterator]",
        0,
        &string_iterator,
        Some(to_primitive),
        string_values,
    )?;
    intrinsics
        .get(IntrinsicId::StringPrototype)
        .define_own_property(
            iterator_key,
            PropertyDescriptor::data(Value::Object(values), true, false, true),
        )?;
    Ok(())
}

pub(super) fn array_values(prototype: &Object, target: Object) -> RuntimeResult<Object> {
    let result = Object::new_for(prototype);
    let target = super::stored::StoredObject::store(&result, &target)?;
    result.initialize_prototype(Some(prototype.clone()));
    result.0.state.borrow_mut().iterator = Some(Box::new(IteratorSlot::Array {
        target: Some(target),
        index: 0,
    }));
    Ok(result)
}

fn string_values(
    prototype: &Object,
    key: Option<&PropertyKey>,
    this: Value,
    _: &[Value],
) -> RuntimeResult<Value> {
    if matches!(this, Value::Undefined | Value::Null) {
        return Err(RuntimeError::type_error(
            "string iterator receiver is null or undefined",
        ));
    }
    let text =
        primitive(&this, key.expect("string iterator primitive key"), true)?.to_js_string()?;
    let result = Object::new_for(prototype);
    result.initialize_prototype(Some(prototype.clone()));
    result.0.state.borrow_mut().iterator = Some(Box::new(IteratorSlot::String {
        text: Some(text),
        index: 0,
    }));
    Ok(Value::Object(result))
}

fn symbol_method(
    holder: &Object,
    prototype: &Object,
    key: PropertyKey,
    name: &str,
    callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
) -> RuntimeResult<()> {
    let function = Object::native_function_for(prototype, callback);
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
    let result = Object::new_for(prototype);
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
    let target = target.load(&iterator)?;
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

#[cfg(test)]
mod managed_tests {
    use super::*;
    use crate::runtime::ArrayObject;

    #[test]
    fn managed_string_factory_capture_survives_conversion_reentry_and_iterator_escape() {
        let runtime = crate::runtime::Runtime::new();
        let function_prototype = Object::new_in(&runtime);
        let iterator_prototype = Object::new_in(&runtime);
        let weak_prototype = iterator_prototype.downgrade();
        let factory = Object::native_function_with_object_capture(
            &function_prototype,
            "[Symbol.iterator]",
            0,
            &iterator_prototype,
            Some("unused".into()),
            string_values,
        )
        .unwrap();
        drop(function_prototype);
        drop(iterator_prototype);
        runtime.collect_migration_graph().unwrap();
        let receiver = Object::new_in(&runtime);
        let callback_runtime = runtime.clone();
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let callback_calls = calls.clone();
        receiver.set_to_string_hook(move || {
            callback_runtime.collect_migration_graph().unwrap();
            let _temporary = Object::new_in(&callback_runtime);
            callback_calls.set(callback_calls.get() + 1);
            Ok(Value::String("💡x".into()))
        });
        let Value::Object(iterator) = factory.call(Value::Object(receiver.clone()), &[]).unwrap()
        else {
            panic!("string iterator");
        };
        assert_eq!(calls.get(), 1);
        drop(receiver);
        drop(factory);
        runtime.collect_migration_graph().unwrap();
        assert_eq!(iterator.prototype(), weak_prototype.upgrade());
        for expected in [Some("💡"), Some("x"), None] {
            let Value::Object(step) = string_next(
                Value::Object(iterator.clone()),
                &iterator.prototype().unwrap(),
            )
            .unwrap() else {
                panic!("iterator result");
            };
            assert_eq!(
                step.get(&"done".into()).unwrap().to_boolean(),
                expected.is_none()
            );
            if let Some(expected) = expected {
                assert!(
                    step.get(&"value".into())
                        .unwrap()
                        .same_value(&Value::String(expected.into()))
                );
            }
        }
        drop(iterator);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak_prototype.upgrade().is_none());
    }

    #[test]
    fn managed_iterator_keeps_target_until_exhaustion_then_releases_it() {
        let runtime = crate::runtime::Runtime::new();
        let prototype = Object::new_in(&runtime);
        let target = ArrayObject::new_for(&prototype, 1).unwrap().object();
        target
            .create_data_property("0".into(), Value::Number(42.0))
            .unwrap();
        let weak = target.downgrade();
        let iterator = Object::new_in(&runtime);
        let stored = super::super::stored::StoredObject::store(&iterator, &target).unwrap();
        iterator.0.state.borrow_mut().iterator = Some(Box::new(IteratorSlot::Array {
            target: Some(stored),
            index: 0,
        }));
        drop(target);
        runtime.collect_migration_graph().unwrap();
        assert!(weak.upgrade().is_some());
        let Value::Object(first) = array_next(
            Value::Object(iterator.clone()),
            &"unused".into(),
            &prototype,
        )
        .unwrap() else {
            panic!("iterator result");
        };
        assert!(
            first
                .get(&"value".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop(first);
        let Value::Object(last) = array_next(
            Value::Object(iterator.clone()),
            &"unused".into(),
            &prototype,
        )
        .unwrap() else {
            panic!("iterator result");
        };
        assert!(last.get(&"done".into()).unwrap().to_boolean());
        drop(last);
        runtime.collect_migration_graph().unwrap();
        assert!(weak.upgrade().is_none());
        drop(iterator);
        drop(prototype);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }
}
