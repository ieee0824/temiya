//! Generic Array stack/queue algorithms. Keep observable Get/Has/Set/Delete
//! operations ordered; indexed storage can optimize those internal methods.

use super::{
    BigIntObject, IntrinsicId, IntrinsicRegistry, JsString, NumberObject, Object,
    PropertyDescriptor, PropertyKey, RuntimeError, RuntimeResult, StringObject, Value,
    WellKnownSymbol, WellKnownSymbols, stored::StoredObject,
};
use std::rc::Rc;

const MAX_LENGTH: u64 = 9_007_199_254_740_991;

struct StoredCoercion {
    string: StoredObject,
    number: StoredObject,
    boolean: StoredObject,
    symbol: StoredObject,
    bigint: StoredObject,
    to_primitive: PropertyKey,
    to_string_tag: PropertyKey,
}
impl StoredCoercion {
    fn new(
        context: &Object,
        intrinsics: &IntrinsicRegistry,
        symbols: &WellKnownSymbols,
    ) -> RuntimeResult<Self> {
        let capture = |id| StoredObject::initial(context.0.runtime(), &intrinsics.get(id));
        Ok(Self {
            string: capture(IntrinsicId::StringPrototype)?,
            number: capture(IntrinsicId::NumberPrototype)?,
            boolean: capture(IntrinsicId::BooleanPrototype)?,
            symbol: capture(IntrinsicId::SymbolPrototype)?,
            bigint: capture(IntrinsicId::BigIntPrototype)?,
            to_primitive: symbols.get(WellKnownSymbol::ToPrimitive).into(),
            to_string_tag: symbols.get(WellKnownSymbol::ToStringTag).into(),
        })
    }
    fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        for object in [
            &self.string,
            &self.number,
            &self.boolean,
            &self.symbol,
            &self.bigint,
        ] {
            object.trace(tracer);
        }
    }
    fn object(&self, owner: &Object, value: Value) -> RuntimeResult<Object> {
        let (object, prototype) = match value {
            Value::Object(object) => return Ok(object),
            Value::Undefined | Value::Null => {
                return Err(RuntimeError::type_error(
                    "array method receiver is null or undefined",
                ));
            }
            Value::String(value) => {
                let prototype = self.string.load(owner)?;
                (StringObject::new_for(&prototype, value).object(), prototype)
            }
            Value::Number(value) => {
                let prototype = self.number.load(owner)?;
                (NumberObject::new_for(&prototype, value).object(), prototype)
            }
            Value::BigInt(value) => {
                let prototype = self.bigint.load(owner)?;
                (BigIntObject::new_for(&prototype, value).object(), prototype)
            }
            value @ Value::Boolean(_) | value @ Value::Symbol(_) => {
                let prototype = if matches!(value, Value::Boolean(_)) {
                    &self.boolean
                } else {
                    &self.symbol
                }
                .load(owner)?;
                let object = Object::new_for(&prototype);
                *object.0.state.borrow_mut().primitive_data_mut() = Some(Box::new(value));
                (object, prototype)
            }
        };
        object.initialize_prototype(Some(prototype));
        Ok(object)
    }
    fn length(&self, object: &Object) -> RuntimeResult<u64> {
        let key: PropertyKey = "length".into();
        let value = match object.own_data_value_with_hint(&key, None) {
            Some((value, _)) => value,
            None => object.get(&key)?,
        };
        let number = match value {
            Value::Number(number) => number,
            value => super::coercion::primitive(&value, &self.to_primitive, false)?.to_number()?,
        };
        Ok((number as u64).min(MAX_LENGTH))
    }
}

enum BuiltinOperation {
    ValueOf,
    ToString,
    Array(Operation),
    Values(StoredObject),
}
pub(super) struct ArrayBuiltin {
    coercion: Rc<StoredCoercion>,
    operation: BuiltinOperation,
}
impl ArrayBuiltin {
    pub(super) fn call(
        &self,
        owner: &Object,
        this: Value,
        arguments: &[Value],
    ) -> RuntimeResult<Value> {
        match &self.operation {
            BuiltinOperation::ValueOf => self.coercion.object(owner, this).map(Value::Object),
            BuiltinOperation::ToString => {
                match this {
                    Value::Undefined => return Ok(Value::String("[object Undefined]".into())),
                    Value::Null => return Ok(Value::String("[object Null]".into())),
                    _ => {}
                }
                let object = self.coercion.object(owner, this)?;
                let tag = object_tag(&object)?;
                let tag = match object.get(&self.coercion.to_string_tag)? {
                    Value::String(tag) => tag,
                    _ => tag.into(),
                };
                let mut units = JsString::from("[object ").code_units().to_vec();
                units.extend_from_slice(tag.code_units());
                units.push(u16::from(b']'));
                Ok(Value::String(JsString::from_code_units(units)))
            }
            BuiltinOperation::Array(operation) => {
                let object = self.coercion.object(owner, this)?;
                let length = self.coercion.length(&object)?;
                operation(&object, length, arguments)
            }
            BuiltinOperation::Values(prototype) => {
                let target = self.coercion.object(owner, this)?;
                super::iterator::array_values(&prototype.load(owner)?, target).map(Value::Object)
            }
        }
    }
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        self.coercion.trace(tracer);
        if let BuiltinOperation::Values(prototype) = &self.operation {
            prototype.trace(tracer);
        }
    }
}
fn create_builtin(
    prototype: &Object,
    name: &str,
    length: u32,
    coercion: Rc<StoredCoercion>,
    operation: BuiltinOperation,
) -> Object {
    Object::managed_function_with_metadata(
        prototype,
        name,
        length,
        super::object::ManagedCallable::Array(ArrayBuiltin {
            coercion,
            operation,
        }),
    )
}
pub(super) fn install(
    intrinsics: &IntrinsicRegistry,
    symbols: &WellKnownSymbols,
) -> RuntimeResult<()> {
    let function = intrinsics.get(IntrinsicId::FunctionPrototype);
    let object = intrinsics.get(IntrinsicId::ObjectPrototype);
    let coercion = Rc::new(StoredCoercion::new(&function, intrinsics, symbols)?);
    for (name, operation) in [
        ("valueOf", BuiltinOperation::ValueOf),
        ("toString", BuiltinOperation::ToString),
    ] {
        let method = create_builtin(&function, name, 0, coercion.clone(), operation);
        object.define_own_property(
            name.into(),
            PropertyDescriptor::data(Value::Object(method), true, false, true),
        )?;
    }
    let array = intrinsics.get(IntrinsicId::ArrayPrototype);
    for (name, arity, operation) in [
        ("push", 1, push as Operation),
        ("pop", 0, pop as Operation),
        ("shift", 0, shift as Operation),
        ("unshift", 1, unshift as Operation),
    ] {
        let method = create_builtin(
            &function,
            name,
            arity,
            coercion.clone(),
            BuiltinOperation::Array(operation),
        );
        array.define_own_property(
            name.into(),
            PropertyDescriptor::data(Value::Object(method), true, false, true),
        )?;
    }
    Ok(())
}
pub(super) fn values_function(
    intrinsics: &IntrinsicRegistry,
    symbols: &WellKnownSymbols,
    iterator_prototype: &Object,
) -> RuntimeResult<Object> {
    let function = intrinsics.get(IntrinsicId::FunctionPrototype);
    let coercion = Rc::new(StoredCoercion::new(&function, intrinsics, symbols)?);
    let prototype = StoredObject::initial(function.0.runtime(), iterator_prototype)?;
    Ok(create_builtin(
        &function,
        "values",
        0,
        coercion,
        BuiltinOperation::Values(prototype),
    ))
}

type Operation = fn(&Object, u64, &[Value]) -> RuntimeResult<Value>;

pub(super) fn method(
    holder: &Object,
    prototype: &Object,
    name: &str,
    length: u32,
    callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
) -> RuntimeResult<()> {
    let function = super::native_function::create(prototype, name, length, callback)?;
    holder.define_own_property(
        name.into(),
        PropertyDescriptor::data(Value::Object(function), true, false, true),
    )?;
    Ok(())
}

fn key(mut index: u64) -> PropertyKey {
    // Array algorithms use nonnegative integer keys, including generic
    // array-like indices beyond u32::MAX. Write decimal UTF-16 directly so
    // short keys use JsString's inline storage without a temporary String.
    let mut units = [0u16; 20];
    let mut start = units.len();
    loop {
        start -= 1;
        units[start] = u16::from(b'0') + (index % 10) as u16;
        index /= 10;
        if index == 0 {
            break;
        }
    }
    JsString::from_code_units_slice(&units[start..]).into()
}

fn set(object: &Object, key: PropertyKey, value: Value) -> RuntimeResult<()> {
    if !object.set(&key, value)? {
        return Err(RuntimeError::type_error(
            "array method could not set property",
        ));
    }
    Ok(())
}

fn length(object: &Object, length: u64) -> RuntimeResult<Value> {
    let value = Value::Number(length as f64);
    set(object, "length".into(), value.clone())?;
    Ok(value)
}

fn delete(object: &Object, index: u64) -> RuntimeResult<()> {
    if !object.try_delete(&key(index))? {
        return Err(RuntimeError::type_error(
            "array method could not delete property",
        ));
    }
    Ok(())
}

fn push(object: &Object, len: u64, items: &[Value]) -> RuntimeResult<Value> {
    let count = items.len() as u64;
    if count > MAX_LENGTH - len {
        return Err(RuntimeError::type_error(
            "array-like length exceeds the safe integer limit",
        ));
    }
    if let Some(end) = object.try_push_array_values(len, items) {
        return Ok(Value::Number(f64::from(end)));
    }
    for (index, item) in items.iter().enumerate() {
        set(object, key(len + index as u64), item.clone())?;
    }
    length(object, len + count)
}

fn pop(object: &Object, len: u64, _: &[Value]) -> RuntimeResult<Value> {
    if len == 0 {
        length(object, 0)?;
        return Ok(Value::Undefined);
    }
    let value = object.get(&key(len - 1))?;
    delete(object, len - 1)?;
    length(object, len - 1)?;
    Ok(value)
}

fn shift(object: &Object, len: u64, _: &[Value]) -> RuntimeResult<Value> {
    if len == 0 {
        length(object, 0)?;
        return Ok(Value::Undefined);
    }
    let value = object.get(&key(0))?;
    for from in 1..len {
        move_property(object, from, from - 1)?;
    }
    delete(object, len - 1)?;
    length(object, len - 1)?;
    Ok(value)
}

fn unshift(object: &Object, len: u64, items: &[Value]) -> RuntimeResult<Value> {
    let count = items.len() as u64;
    if count > MAX_LENGTH - len {
        return Err(RuntimeError::type_error(
            "array-like length exceeds the safe integer limit",
        ));
    }
    if count != 0 {
        for from in (0..len).rev() {
            move_property(object, from, from + count)?;
        }
        for (index, item) in items.iter().enumerate() {
            set(object, key(index as u64), item.clone())?;
        }
    }
    length(object, len + count)
}

fn move_property(object: &Object, from: u64, to: u64) -> RuntimeResult<()> {
    let from = key(from);
    if object.has_property(&from)? {
        set(object, key(to), object.get(&from)?)
    } else {
        delete(object, to)
    }
}

fn is_array(object: &Object) -> RuntimeResult<bool> {
    let mut object = object.clone();
    loop {
        let state = object.0.state.borrow();
        if state.array {
            return Ok(true);
        }
        let proxy = state.proxy().clone();
        drop(state);
        let Some(proxy) = proxy else {
            return Ok(false);
        };
        object = proxy.target(&object)?;
    }
}

fn object_tag(object: &Object) -> RuntimeResult<&'static str> {
    if is_array(object)? {
        return Ok("Array");
    }
    if object.is_callable() {
        return Ok("Function");
    }
    let state = object.0.state.borrow();
    Ok(if state.string_data().is_some() {
        "String"
    } else if state.date_value().is_some() {
        "Date"
    } else if state.regexp().is_some() {
        "RegExp"
    } else {
        match state.primitive_data().as_deref() {
            Some(Value::Number(_)) => "Number",
            Some(Value::Boolean(_)) => "Boolean",
            _ => "Object",
        }
    })
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn managed_coercion_captures_are_traced_and_boxed_values_keep_their_prototypes() {
        let runtime = crate::runtime::Runtime::new();
        let function_prototype = Object::new_in(&runtime);
        let prototypes: [Object; 5] = std::array::from_fn(|_| Object::new_in(&runtime));
        let captured = |index: usize| {
            StoredObject::initial(function_prototype.0.runtime(), &prototypes[index]).unwrap()
        };
        let coercion = Rc::new(StoredCoercion {
            string: captured(0),
            number: captured(1),
            boolean: captured(2),
            symbol: captured(3),
            bigint: captured(4),
            to_primitive: "unused".into(),
            to_string_tag: "unusedTag".into(),
        });
        let method = create_builtin(
            &function_prototype,
            "valueOf",
            0,
            coercion,
            BuiltinOperation::ValueOf,
        );
        for prototype in &prototypes {
            prototype
                .create_data_property("back".into(), Value::Object(method.clone()))
                .unwrap();
        }
        let weak = prototypes.map(|prototype| prototype.downgrade());
        drop(function_prototype);
        assert_eq!(runtime.diagnostics().rooted, 1);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 7);
        let Value::Object(boxed) = method.call(Value::Number(42.0), &[]).unwrap() else {
            panic!("boxed number");
        };
        assert_eq!(boxed.prototype(), weak[1].upgrade());
        assert!(matches!(
            boxed.0.state.borrow().primitive_data().as_deref(),
            Some(Value::Number(42.0))
        ));
        drop(method);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 8);
        drop(boxed);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.iter().all(|prototype| prototype.upgrade().is_none()));
    }

    #[test]
    fn integer_keys_preserve_decimal_boundaries_without_number_rounding() {
        for (index, expected) in [
            (0, "0"),
            (9, "9"),
            (10, "10"),
            (u64::from(u32::MAX), "4294967295"),
            (MAX_LENGTH, "9007199254740991"),
            (u64::MAX, "18446744073709551615"),
        ] {
            assert_eq!(key(index), PropertyKey::from(expected));
        }
        let mut power = 10u64;
        loop {
            for index in [power - 1, power, power + 1] {
                assert_eq!(
                    key(index),
                    PropertyKey::from(JsString::from(index.to_string()))
                );
            }
            let Some(next) = power.checked_mul(10) else {
                break;
            };
            power = next;
        }
    }
}
