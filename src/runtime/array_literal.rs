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
    // Appending array indices already maintains length. Defining the same
    // numeric value has no observable effect on an ordinary array, including
    // a non-writable length. Holes, overflow and exotic objects use Define.
    if object
        .ordinary_array_length_info()
        .is_some_and(|(length, _)| u64::from(length) == next)
    {
        return Ok(());
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{AllocationContext, RuntimeErrorKind};

    #[test]
    fn finishing_preserves_readonly_lengths_holes_and_overflow_errors() {
        let context = AllocationContext::new();
        let array = ArrayObject::new(&context, 2).unwrap().object();
        array
            .define_own_property(
                "length".into(),
                PropertyDescriptor {
                    writable: Some(false),
                    ..PropertyDescriptor::default()
                },
            )
            .unwrap();
        finish(&array, 2).unwrap();
        assert_eq!(
            array.get_own_property(&"length".into()).unwrap().writable,
            Some(false)
        );
        assert_eq!(
            finish(&array, 3).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
        let holes = ArrayObject::new(&context, 0).unwrap().object();
        finish(&holes, 3).unwrap();
        assert_eq!(holes.array_length(), 3);
        assert!(holes.get_own_property(&"0".into()).is_none());
        assert_eq!(
            finish(&holes, u64::from(u32::MAX) + 1).unwrap_err().kind(),
            RuntimeErrorKind::RangeError
        );
    }
}
