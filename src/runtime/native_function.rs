//! Realm-specific native callable allocation; no Realm is captured by the callback wrapper.
use super::{Object, PropertyDescriptor, RuntimeResult, Value};
pub(super) fn create(
    prototype: &Object,
    name: &str,
    length: u32,
    callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
) -> RuntimeResult<Object> {
    let function = Object::native_function(callback);
    function.initialize_prototype(Some(prototype.clone()));
    function.define_own_property(
        "name".into(),
        PropertyDescriptor::data(Value::String(name.into()), false, false, true),
    )?;
    function.define_own_property(
        "length".into(),
        PropertyDescriptor::data(Value::Number(f64::from(length)), false, false, true),
    )?;
    Ok(function)
}
