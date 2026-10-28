//! Realm-specific native callable allocation with traced source Realm metadata.
use super::{Object, RuntimeResult, Value};
pub(super) fn create(
    prototype: &Object,
    name: &str,
    length: u32,
    callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
) -> RuntimeResult<Object> {
    Ok(Object::native_function_with_metadata(
        prototype, name, length, callback,
    ))
}
