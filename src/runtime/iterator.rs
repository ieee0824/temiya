//! Shared synchronous iterator operations for array and argument consumers.

use super::{
    Completion, IntrinsicId, IntrinsicRegistry, JsString, Object, PropertyDescriptor, PropertyKey,
    Realm, Reference, ReferencedName, RuntimeError, RuntimeErrorKind, RuntimeResult, Value,
    WellKnownSymbol, WellKnownSymbols, coercion::primitive,
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

/// A rooted record: no heap-state borrow survives an observable operation.
/// Private consumers can reuse this record for stepping and closing separately.
pub(crate) struct IteratorRecord {
    iterator: Object,
    next_method: Value,
    done: bool,
    done_key: PropertyKey,
    value_key: PropertyKey,
}

impl IteratorRecord {
    pub(super) fn is_done(&self) -> bool {
        self.done
    }

    pub(crate) fn get(realm: &Realm, value: Value) -> RuntimeResult<Self> {
        let method = Reference::property_in_realm(
            realm,
            value.clone(),
            ReferencedName::PropertyKey(realm.well_known_symbol(WellKnownSymbol::Iterator).into()),
            false,
        )
        .get_value()?;
        let Value::Object(method) = method else {
            return Err(RuntimeError::type_error("value is not iterable"));
        };
        let Value::Object(iterator) = method.call(value, &[])? else {
            return Err(RuntimeError::type_error(
                "iterator method returned a non-object",
            ));
        };
        // GetIterator captures next once; callability is checked by IteratorNext.
        let next_method = iterator.get(&"next".into())?;
        Ok(Self {
            iterator,
            next_method,
            done: false,
            done_key: "done".into(),
            value_key: "value".into(),
        })
    }

    pub(crate) fn next(&mut self, value: Option<Value>) -> RuntimeResult<Object> {
        let result = match &self.next_method {
            Value::Object(method) => {
                method.call(Value::Object(self.iterator.clone()), value.as_slice())
            }
            _ => Err(RuntimeError::type_error("iterator next is not callable")),
        };
        match result {
            Ok(Value::Object(result)) => Ok(result),
            Ok(_) => {
                self.done = true;
                Err(RuntimeError::type_error(
                    "iterator next returned a non-object",
                ))
            }
            Err(error) => {
                self.done = true;
                Err(error)
            }
        }
    }

    pub(crate) fn step(&mut self) -> RuntimeResult<Option<Object>> {
        let result = self.next(None)?;
        match result.get(&self.done_key) {
            Ok(done) if done.to_boolean() => {
                self.done = true;
                Ok(None)
            }
            Ok(_) => Ok(Some(result)),
            Err(error) => {
                self.done = true;
                Err(error)
            }
        }
    }

    pub(crate) fn step_value(&mut self) -> RuntimeResult<Option<Value>> {
        if let Value::Object(method) = &self.next_method {
            if let Some(result) = method.try_builtin_iterator_step(&self.iterator) {
                if !matches!(&result, Ok(Some(_))) {
                    self.done = true;
                }
                return result;
            }
        }
        let Some(result) = self.step()? else {
            return Ok(None);
        };
        match result.get(&self.value_key) {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                self.done = true;
                Err(error)
            }
        }
    }

    pub(crate) fn close(&self, completion: RuntimeResult<Completion>) -> RuntimeResult<Completion> {
        // Internal host/invariant errors are never converted to JS completions
        // or made observable through a JavaScript cleanup callback.
        if matches!(&completion, Err(error) if error.kind() == RuntimeErrorKind::InternalError) {
            return completion;
        }
        let inner = (|| {
            let method = self.iterator.get(&"return".into())?;
            if matches!(method, Value::Undefined | Value::Null) {
                return Ok(None);
            }
            let Value::Object(method) = method else {
                return Err(RuntimeError::type_error("iterator return is not callable"));
            };
            method
                .call(Value::Object(self.iterator.clone()), &[])
                .map(Some)
        })();
        let inner = match inner {
            Err(error) if error.kind() == RuntimeErrorKind::InternalError => return Err(error),
            inner => inner,
        };
        // A pre-existing JS throw outranks return lookup/call/result failures.
        if matches!(&completion, Err(_) | Ok(Completion::Throw(_))) {
            return completion;
        }
        match inner? {
            None | Some(Value::Object(_)) => completion,
            Some(_) => Err(RuntimeError::type_error(
                "iterator return returned a non-object",
            )),
        }
    }
}

/// Consume a synchronous iterator without materializing an intermediate list.
pub(crate) fn consume_iterable(
    realm: &Realm,
    value: Value,
    mut append: impl FnMut(Value) -> RuntimeResult<()>,
) -> RuntimeResult<()> {
    let mut record = IteratorRecord::get(realm, value)?;
    while !record.done {
        let Some(value) = record.step_value()? else {
            break;
        };
        if let Err(error) = append(value) {
            return record.close(Err(error)).map(|_| ());
        }
    }
    Ok(())
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
        array_next_call,
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
        string_next_call,
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
    Object::iterator_result(prototype, value, done).map(Value::Object)
}

pub(super) fn array_next_call(
    prototype: &Object,
    key: Option<&PropertyKey>,
    this: Value,
    _: &[Value],
) -> RuntimeResult<Value> {
    array_next(this, key.expect("iterator primitive key"), prototype)
}

fn array_next(this: Value, key: &PropertyKey, prototype: &Object) -> RuntimeResult<Value> {
    let value = array_step_value(this, key)?;
    let done = value.is_none();
    result(prototype, value.unwrap_or(Value::Undefined), done)
}

pub(super) fn array_step_value(
    this: Value,
    to_primitive: &PropertyKey,
) -> RuntimeResult<Option<Value>> {
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
        return Ok(None);
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
        return Ok(None);
    }
    // The captured index is advanced after length coercion, before element Get,
    // matching reentrant getters and abrupt element access without RefCell borrows.
    if let Some(IteratorSlot::Array { index: next, .. }) =
        iterator.0.state.borrow_mut().iterator.as_deref_mut()
    {
        *next = index + 1;
    }
    let value = target.get(&PropertyKey::integer_index(index))?;
    Ok(Some(value))
}

pub(super) fn string_next_call(
    prototype: &Object,
    _: Option<&PropertyKey>,
    this: Value,
    _: &[Value],
) -> RuntimeResult<Value> {
    string_next(this, prototype)
}

fn string_next(this: Value, prototype: &Object) -> RuntimeResult<Value> {
    let value = string_step_value(this)?;
    let done = value.is_none();
    result(prototype, value.unwrap_or(Value::Undefined), done)
}

pub(super) fn string_step_value(this: Value) -> RuntimeResult<Option<Value>> {
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
    Ok(value)
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

#[cfg(test)]
mod operation_tests {
    use super::*;
    use crate::{parser::parse_script, runtime::StatementEvaluator};

    fn evaluate(realm: &Realm, source: &str) -> Value {
        let Completion::Normal(Some(value)) = StatementEvaluator::new(realm.clone())
            .evaluate_program(&parse_script(source).unwrap())
            .unwrap()
        else {
            panic!("normal value");
        };
        value
    }
    fn record(source: &str) -> (Realm, IteratorRecord) {
        let context = crate::runtime::AllocationContext::new();
        let realm = Realm::ordinary_in(&context);
        let iterable = evaluate(&realm, source);
        let record = IteratorRecord::get(&realm, iterable).unwrap();
        (realm, record)
    }

    #[test]
    fn captured_next_keeps_receiver_and_distinguishes_missing_from_undefined() {
        let (realm, mut record) = record(
            "var reads=0;var receiverOK=true;var iterator;var iterable={[Symbol.iterator](){iterator={get next(){reads++;return function(){receiverOK=receiverOK&&this===iterator;return {done:false,value:arguments.length};};}};return iterator;}};iterable;",
        );
        assert!(
            record
                .next(None)
                .unwrap()
                .get(&"value".into())
                .unwrap()
                .same_value(&Value::Number(0.0))
        );
        assert!(
            record
                .iterator
                .create_data_property("next".into(), Value::Number(0.0))
                .unwrap()
        );
        assert!(
            record
                .next(Some(Value::Undefined))
                .unwrap()
                .get(&"value".into())
                .unwrap()
                .same_value(&Value::Number(1.0))
        );
        assert!(evaluate(&realm, "reads===1&&receiverOK;").same_value(&Value::Boolean(true)));
    }

    #[test]
    fn step_failures_mark_done_and_do_not_read_value_after_completion() {
        for (body, kind) in [
            ("throw 17;", RuntimeErrorKind::ThrownValue),
            ("return 0;", RuntimeErrorKind::TypeError),
            (
                "return {get done(){throw 17;}};",
                RuntimeErrorKind::ThrownValue,
            ),
            (
                "return {done:false,get value(){throw 17;}};",
                RuntimeErrorKind::ThrownValue,
            ),
        ] {
            let (_realm, mut iterator) = record(&format!(
                "({{[Symbol.iterator](){{return {{next(){{{body}}}}};}}}});"
            ));
            assert_eq!(iterator.step_value().unwrap_err().kind(), kind);
            assert!(iterator.done);
        }
        let (_realm, mut iterator) = record(
            "({[Symbol.iterator](){return {next(){return {done:true,get value(){throw 99;}};}};}});",
        );
        assert!(iterator.step_value().unwrap().is_none());
        assert!(iterator.done);
    }

    #[test]
    fn close_preserves_throw_but_validates_normal_return_and_its_result() {
        for (property, kind) in [
            ("get return(){throw 17;}", RuntimeErrorKind::ThrownValue),
            (
                "return:function(){throw 17;}",
                RuntimeErrorKind::ThrownValue,
            ),
            ("return:0", RuntimeErrorKind::TypeError),
            ("return:function(){return 0;}", RuntimeErrorKind::TypeError),
        ] {
            let (_realm, iterator) = record(&format!(
                "({{[Symbol.iterator](){{return {{next(){{return {{done:false}};}},{property}}};}}}});"
            ));
            let result = iterator
                .close(Ok(Completion::Throw(Value::Number(99.0))))
                .unwrap();
            assert!(matches!(result, Completion::Throw(Value::Number(99.0))));
            let error = iterator
                .close(Err(RuntimeError::thrown(Value::Number(99.0))))
                .unwrap_err();
            assert!(
                error
                    .thrown_value()
                    .unwrap()
                    .same_value(&Value::Number(99.0))
            );
            assert_eq!(
                iterator
                    .close(Ok(Completion::Return(Value::Number(42.0))))
                    .unwrap_err()
                    .kind(),
                kind
            );
        }
        for property in [
            "return:undefined",
            "return:null",
            "return:function(){return {};}",
        ] {
            let (_realm, iterator) = record(&format!(
                "({{[Symbol.iterator](){{return {{next(){{return {{done:false}};}},{property}}};}}}});"
            ));
            let result = iterator
                .close(Ok(Completion::Break {
                    value: Some(Value::Number(42.0)),
                    target: Some("outer".into()),
                }))
                .unwrap();
            assert!(
                matches!(result,Completion::Break{value:Some(Value::Number(42.0)),target:Some(label)} if label=="outer")
            );
        }
    }

    #[test]
    fn internal_failures_neither_run_cleanup_nor_get_hidden_by_javascript_throw() {
        let (_realm, iterator) =
            record("({[Symbol.iterator](){return {next(){return {done:false};}};}});");
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let observed = calls.clone();
        let method =
            Object::native_function_in(iterator.iterator.allocation_context(), move |_, _| {
                observed.set(observed.get() + 1);
                Err(RuntimeError::internal_error("host failure"))
            });
        iterator
            .iterator
            .create_data_property("return".into(), Value::Object(method))
            .unwrap();
        assert_eq!(
            iterator
                .close(Err(RuntimeError::internal_error("original host failure")))
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::InternalError
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(
            iterator
                .close(Ok(Completion::Throw(Value::Number(99.0))))
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::InternalError
        );
        assert_eq!(calls.get(), 1);
    }
}
