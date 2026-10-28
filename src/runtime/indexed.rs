//! Array exotic objects and byte-backed buffer views.

use std::{cell::RefCell, rc::Rc};

use super::{JsString, Object, PropertyDescriptor, RuntimeError, RuntimeResult, Value};

pub(super) type BufferSlot = Rc<RefCell<Option<Vec<u8>>>>;

#[derive(Clone)]
pub(super) struct ViewSlot {
    pub buffer: BufferSlot,
    pub byte_offset: usize,
    pub byte_length: usize,
}

/// An Array exotic object that enforces the `length` property invariant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArrayObject(Object);

impl ArrayObject {
    /// Creates an empty Array with the requested length; indices initially remain holes.
    pub fn new(context: &super::AllocationContext, length: u32) -> RuntimeResult<Self> {
        Self::initialize(Object::new_in(context), length)
    }

    pub(crate) fn new_for(context: &Object, length: u32) -> RuntimeResult<Self> {
        Self::initialize(Object::new_for(context), length)
    }

    pub(super) fn initialize(object: Object, length: u32) -> RuntimeResult<Self> {
        object.0.state.borrow_mut().array = true;
        object.0.state.borrow_mut().elements = Some(Box::default());
        object.define_own_property(
            "length".into(),
            PropertyDescriptor::data(Value::Number(f64::from(length)), true, false, false),
        )?;
        Ok(Self(object))
    }

    /// Wraps an object when it has Array exotic semantics.
    #[must_use]
    pub fn from_object(object: Object) -> Option<Self> {
        let is_array = object.0.state.borrow().array;
        is_array.then_some(Self(object))
    }

    /// Returns the underlying JavaScript object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }

    /// Returns the current array length.
    #[must_use]
    pub fn length(&self) -> u32 {
        self.0.array_length()
    }

    /// Returns whether an indexed element is present, preserving holes.
    #[must_use]
    pub fn has_index(&self, index: u32) -> bool {
        self.0
            .get_own_property(&JsString::from(index.to_string()).into())
            .is_some()
    }

    /// Copies a range while preserving holes, optionally using a species constructor.
    pub fn slice_with_species(
        &self,
        start: u32,
        end: u32,
        species: Option<&Object>,
    ) -> RuntimeResult<Self> {
        let end = end.min(self.length());
        let start = start.min(end);
        let length = end - start;
        let result = if let Some(constructor) = species {
            if !constructor.is_constructor() {
                return Err(RuntimeError::type_error(
                    "array species is not a constructor",
                ));
            }
            let object =
                constructor.construct(&[Value::Number(f64::from(length))], constructor.clone())?;
            Self::from_object(object)
                .ok_or_else(|| RuntimeError::type_error("array species did not create an Array"))?
        } else {
            Self::new_for(&self.0, length)?
        };
        for offset in 0..length {
            let source = JsString::from((start + offset).to_string()).into();
            if self.0.get_own_property(&source).is_some() {
                result.0.create_data_property(
                    JsString::from(offset.to_string()).into(),
                    self.0.get(&source)?,
                )?;
            }
        }
        Ok(result)
    }
}

/// A detachable, fixed-length byte buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArrayBuffer(Object);

impl ArrayBuffer {
    /// Allocates a zero-filled buffer.
    pub fn new(context: &super::AllocationContext, byte_length: usize) -> RuntimeResult<Self> {
        Self::allocate(|| Object::new_in(context), byte_length)
    }

    pub(crate) fn new_for(context: &Object, byte_length: usize) -> RuntimeResult<Self> {
        Self::allocate(|| Object::new_for(context), byte_length)
    }

    fn allocate(make_object: impl FnOnce() -> Object, byte_length: usize) -> RuntimeResult<Self> {
        if byte_length > 100_000_000 {
            return Err(RuntimeError::range_error("ArrayBuffer is too large"));
        }
        let object = make_object();
        object.0.state.borrow_mut().buffer =
            Some(Rc::new(RefCell::new(Some(vec![0; byte_length]))));
        Ok(Self(object))
    }

    /// Wraps an object when it has ArrayBuffer internal slots.
    #[must_use]
    pub fn from_object(object: Object) -> Option<Self> {
        object.0.state.borrow().buffer.as_ref()?;
        Some(Self(object))
    }

    /// Returns the underlying JavaScript object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }

    /// Returns the byte length, or zero after detachment.
    #[must_use]
    pub fn byte_length(&self) -> usize {
        self.slot().borrow().as_ref().map_or(0, Vec::len)
    }

    /// Returns whether the buffer has been detached.
    #[must_use]
    pub fn is_detached(&self) -> bool {
        self.slot().borrow().is_none()
    }

    /// Detaches this buffer and invalidates all of its views.
    pub fn detach(&self) {
        *self.slot().borrow_mut() = None;
    }

    fn slot(&self) -> BufferSlot {
        self.0
            .0
            .state
            .borrow()
            .buffer
            .clone()
            .expect("ArrayBuffer slot")
    }
}

/// A Uint8 typed array view over an [`ArrayBuffer`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedArray(Object);

impl TypedArray {
    /// Creates a Uint8 view, checking offset and length against the buffer.
    pub fn new(buffer: &ArrayBuffer, byte_offset: usize, length: usize) -> RuntimeResult<Self> {
        checked_range(buffer, byte_offset, length)?;
        let object = Object::new_for(&buffer.0);
        object.0.state.borrow_mut().view = Some(ViewSlot {
            buffer: buffer.slot(),
            byte_offset,
            byte_length: length,
        });
        Ok(Self(object))
    }

    /// Returns the underlying JavaScript object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }

    /// Wraps an object when it has typed-array view slots.
    #[must_use]
    pub fn from_object(object: Object) -> Option<Self> {
        let is_view = object.0.state.borrow().view.is_some();
        is_view.then_some(Self(object))
    }

    /// Returns the number of Uint8 elements, or an error after detachment.
    pub fn length(&self) -> RuntimeResult<usize> {
        Ok(self.bytes()?.byte_length)
    }

    /// Reads an element; an out-of-range index produces `undefined`.
    pub fn get(&self, index: usize) -> RuntimeResult<Value> {
        let view = self.bytes()?;
        if index >= view.byte_length {
            return Ok(Value::Undefined);
        }
        let bytes = view.buffer.borrow();
        let bytes = bytes.as_ref().ok_or_else(detached)?;
        Ok(Value::Number(f64::from(bytes[view.byte_offset + index])))
    }

    /// Writes an element, rejecting detached or out-of-bounds access.
    pub fn set(&self, index: usize, value: u8) -> RuntimeResult<()> {
        let view = self.bytes()?;
        if index >= view.byte_length {
            return Err(RuntimeError::range_error(
                "typed array index is out of bounds",
            ));
        }
        let mut bytes = view.buffer.borrow_mut();
        let bytes = bytes.as_mut().ok_or_else(detached)?;
        bytes[view.byte_offset + index] = value;
        Ok(())
    }

    /// Creates a view over the same buffer, preserving the current view type.
    pub fn subarray(&self, begin: usize, end: usize) -> RuntimeResult<Self> {
        let view = self.bytes()?;
        if begin > end || end > view.byte_length {
            return Err(RuntimeError::range_error("subarray range is invalid"));
        }
        let buffer_object = Object::new_for(&self.0);
        buffer_object.0.state.borrow_mut().buffer = Some(view.buffer.clone());
        Self::new(
            &ArrayBuffer(buffer_object),
            view.byte_offset + begin,
            end - begin,
        )
    }

    fn bytes(&self) -> RuntimeResult<ViewSlot> {
        let view = self
            .0
            .0
            .state
            .borrow()
            .view
            .clone()
            .ok_or_else(|| RuntimeError::type_error("receiver is not a TypedArray"))?;
        if view.buffer.borrow().is_none() {
            return Err(detached());
        }
        Ok(view)
    }
}

/// A byte-oriented DataView over an [`ArrayBuffer`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataView(TypedArray);

impl DataView {
    /// Creates a checked DataView range.
    pub fn new(
        buffer: &ArrayBuffer,
        byte_offset: usize,
        byte_length: usize,
    ) -> RuntimeResult<Self> {
        TypedArray::new(buffer, byte_offset, byte_length).map(Self)
    }

    /// Returns the underlying JavaScript object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.object()
    }

    /// Reads one unsigned byte.
    pub fn get_uint8(&self, offset: usize) -> RuntimeResult<u8> {
        match self.0.get(offset)? {
            Value::Number(value) => Ok(value as u8),
            _ => Err(RuntimeError::range_error(
                "DataView offset is out of bounds",
            )),
        }
    }

    /// Writes one unsigned byte.
    pub fn set_uint8(&self, offset: usize, value: u8) -> RuntimeResult<()> {
        self.0.set(offset, value)
    }
}

fn checked_range(buffer: &ArrayBuffer, offset: usize, length: usize) -> RuntimeResult<()> {
    if buffer.is_detached() {
        return Err(detached());
    }
    if offset
        .checked_add(length)
        .is_none_or(|end| end > buffer.byte_length())
    {
        return Err(RuntimeError::range_error("buffer view is out of bounds"));
    }
    Ok(())
}

fn detached() -> RuntimeError {
    RuntimeError::type_error("ArrayBuffer is detached")
}

#[cfg(test)]
mod managed_tests {
    use super::*;

    #[test]
    fn managed_views_share_storage_after_buffer_collection_and_detachment() {
        let runtime = crate::runtime::Runtime::new();
        let context = Object::new_in(&runtime);
        let buffer = ArrayBuffer::new_for(&context, 8).unwrap();
        let weak_buffer = buffer.object().downgrade();
        let view = TypedArray::new(&buffer, 1, 4).unwrap();
        let data = DataView::new(&buffer, 1, 4).unwrap();
        view.set(0, 42).unwrap();
        assert_eq!(data.get_uint8(0).unwrap(), 42);
        drop(buffer);
        drop(context);
        runtime.collect_migration_graph().unwrap();
        assert!(weak_buffer.upgrade().is_none());
        assert_eq!(data.get_uint8(0).unwrap(), 42);
        data.set_uint8(0, 43).unwrap();
        assert!(view.get(0).unwrap().same_value(&Value::Number(43.0)));
        let slice = view.subarray(0, 2).unwrap();
        assert!(slice.object().0.runtime().unwrap().same_heap(&runtime));
        slice.set(1, 44).unwrap();
        assert!(view.get(1).unwrap().same_value(&Value::Number(44.0)));
        runtime.collect_migration_graph().unwrap();
        let backing = view.bytes().unwrap().buffer;
        backing.borrow_mut().take();
        assert!(view.get(0).is_err());
        assert!(slice.get(0).is_err());
        assert!(data.get_uint8(0).is_err());
        drop(backing);
        drop(view);
        drop(data);
        drop(slice);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }
}
