//! Array-initializer accumulation shared by AST and VM.
use super::{
    ArrayObject, IntrinsicId, Object, PropertyDescriptor, Realm, RuntimeError, RuntimeResult, Value,
};

pub(crate) fn create(realm: &Realm) -> RuntimeResult<Object> {
    let object = ArrayObject::new_for(&realm.global_object(), 0)?.object();
    object.initialize_prototype(Some(realm.intrinsic(IntrinsicId::ArrayPrototype)));
    Ok(object)
}
// An unpublished AST literal has no identity observer until a value is stored
// or the finished array escapes. Keep yielded values rooted while materializing
// the array, and keep the shared iterator consumer's abrupt-close behavior.
pub(crate) fn materialize<'a>(
    realm: &Realm,
    object: &'a mut Option<Object>,
) -> RuntimeResult<&'a Object> {
    if object.is_none() {
        *object = Some(create(realm)?);
    }
    Ok(object.as_ref().expect("materialized array literal"))
}
pub(crate) fn spread_deferred(
    realm: &Realm,
    object: &mut Option<Object>,
    next: &mut u64,
    value: Value,
) -> RuntimeResult<()> {
    super::iterator::consume_iterable(realm, value, |value| {
        append(materialize(realm, object)?, next, value)
    })
}
pub(crate) fn elide(next: &mut u64) -> RuntimeResult<()> {
    if *next >= 9_007_199_254_740_991 {
        return Err(RuntimeError::range_error(
            "array initializer index exceeds safe integer range",
        ));
    }
    *next += 1;
    Ok(())
}
pub(crate) fn append(object: &Object, next: &mut u64, value: Value) -> RuntimeResult<()> {
    if !object.create_data_property(super::PropertyKey::integer_index(*next), value)? {
        return Err(RuntimeError::type_error(
            "array initializer property was rejected",
        ));
    }
    elide(next)
}
pub(crate) fn spread(
    realm: &Realm,
    object: &Object,
    next: &mut u64,
    value: Value,
) -> RuntimeResult<()> {
    super::iterator::consume_iterable(realm, value, |value| append(object, next, value))
}
pub(crate) fn finish(object: &Object, next: u64) -> RuntimeResult<()> {
    if !object.define_own_property(
        "length".into(),
        PropertyDescriptor {
            value: Some(Value::Number(next as f64)),
            ..PropertyDescriptor::default()
        },
    )? {
        return Err(RuntimeError::type_error(
            "array initializer length was rejected",
        ));
    }
    Ok(())
}
