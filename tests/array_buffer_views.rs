use temiya::runtime::{
    ArrayBuffer, ArrayObject, DataView, IntrinsicId, JsString, PropertyDescriptor, Realm,
    RuntimeErrorKind, TypedArray, Value,
};

#[test]
fn array_length_tracks_indices_holes_and_shrinking() {
    let array = ArrayObject::new(3).unwrap();
    assert_eq!(array.length(), 3);
    assert!(!array.has_index(1));
    array
        .object()
        .create_data_property(JsString::from("5").into(), Value::Number(5.0))
        .unwrap();
    assert_eq!(array.length(), 6);
    array
        .object()
        .define_own_property(
            "5".into(),
            PropertyDescriptor::data(Value::Number(5.0), true, true, false),
        )
        .unwrap();
    assert!(
        !array
            .object()
            .define_own_property(
                "length".into(),
                PropertyDescriptor {
                    value: Some(Value::Number(2.0)),
                    ..PropertyDescriptor::default()
                },
            )
            .unwrap()
    );
    assert_eq!(array.length(), 6);
}

#[test]
fn array_slice_preserves_holes_and_honors_species() {
    let source = ArrayObject::new(3).unwrap();
    source
        .object()
        .create_data_property("1".into(), Value::String("middle".into()))
        .unwrap();
    let species = ObjectSpecies::constructor();
    let result = source.slice_with_species(0, 3, Some(&species)).unwrap();
    assert_eq!(result.length(), 3);
    assert!(!result.has_index(0));
    assert!(result.has_index(1));
    assert!(!result.has_index(2));
}

struct ObjectSpecies;

impl ObjectSpecies {
    fn constructor() -> temiya::runtime::Object {
        temiya::runtime::Object::native_constructor(|arguments, _| {
            let length = match arguments.first() {
                Some(Value::Number(value)) => *value as u32,
                _ => 0,
            };
            Ok(ArrayObject::new(length)?.object())
        })
    }
}

#[test]
fn typed_array_and_data_view_share_bytes_and_validate_bounds() {
    let buffer = ArrayBuffer::new(4).unwrap();
    let typed = TypedArray::new(&buffer, 1, 2).unwrap();
    let view = DataView::new(&buffer, 0, 4).unwrap();
    typed.set(0, 255).unwrap();
    assert_eq!(view.get_uint8(1).unwrap(), 255);
    assert_eq!(typed.length().unwrap(), 2);
    assert_eq!(
        TypedArray::new(&buffer, 3, 2).unwrap_err().kind(),
        RuntimeErrorKind::RangeError
    );
    assert_eq!(
        typed.set(2, 0).unwrap_err().kind(),
        RuntimeErrorKind::RangeError
    );
}

#[test]
fn detachment_invalidates_every_existing_view() {
    let buffer = ArrayBuffer::new(2).unwrap();
    let typed = TypedArray::new(&buffer, 0, 2).unwrap();
    let view = DataView::new(&buffer, 0, 2).unwrap();
    buffer.detach();
    assert!(buffer.is_detached());
    assert_eq!(buffer.byte_length(), 0);
    assert_eq!(
        typed.get(0).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(
        view.get_uint8(0).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn realm_constructors_create_indexed_exotic_objects() {
    let realm = Realm::ordinary();
    let array_constructor = realm.intrinsic(IntrinsicId::Array);
    let array = array_constructor
        .construct(&[Value::Number(2.0)], array_constructor.clone())
        .unwrap();
    assert_eq!(ArrayObject::from_object(array).unwrap().length(), 2);

    let buffer_constructor = realm.intrinsic(IntrinsicId::ArrayBuffer);
    let buffer = buffer_constructor
        .construct(&[Value::Number(8.0)], buffer_constructor.clone())
        .unwrap();
    assert_eq!(ArrayBuffer::from_object(buffer).unwrap().byte_length(), 8);
}

#[test]
fn invalid_lengths_are_rejected() {
    assert_eq!(ArrayObject::new(0).unwrap().length(), 0);
    let realm = Realm::ordinary();
    let array = realm.intrinsic(IntrinsicId::Array);
    assert_eq!(
        array
            .construct(&[Value::Number(1.5)], array.clone())
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::RangeError
    );
}
