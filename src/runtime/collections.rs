//! Insertion-ordered and weak-keyed collection objects.

use std::{cell::RefCell, rc::Rc};

use super::{Object, RuntimeError, RuntimeResult, Value, WeakObject};

type MapEntries = Vec<Option<(Value, Value)>>;
type SetEntries = Vec<Option<Value>>;
pub(super) type MapSlot = Rc<RefCell<MapEntries>>;
pub(super) type SetSlot = Rc<RefCell<SetEntries>>;
pub(super) type WeakMapSlot = Rc<RefCell<Vec<(WeakObject, Value)>>>;
pub(super) type WeakSetSlot = Rc<RefCell<Vec<WeakObject>>>;

/// An insertion-ordered Map using SameValueZero key equality.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapObject(Object);

impl MapObject {
    /// Creates an empty Map.
    #[must_use]
    pub fn new() -> Self {
        let object = Object::new();
        object.0.state.borrow_mut().map_data = Some(Rc::new(RefCell::new(Vec::new())));
        Self(object)
    }

    /// Wraps an object with Map internal slots.
    #[must_use]
    pub fn from_object(object: Object) -> Option<Self> {
        let present = object.0.state.borrow().map_data.is_some();
        present.then_some(Self(object))
    }

    /// Returns the underlying object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }

    /// Inserts or replaces a key while retaining its original order.
    pub fn set(&self, key: Value, value: Value) {
        let slot = self.slot();
        let mut entries = slot.borrow_mut();
        if let Some((_, existing)) = entries
            .iter_mut()
            .flatten()
            .find(|(existing, _)| existing.same_value_zero(&key))
        {
            *existing = value;
        } else {
            entries.push(Some((normalize_zero(key), value)));
        }
    }

    /// Returns a value for a key, or `undefined` when absent.
    #[must_use]
    pub fn get(&self, key: &Value) -> Value {
        self.slot()
            .borrow()
            .iter()
            .flatten()
            .find(|(existing, _)| existing.same_value_zero(key))
            .map_or(Value::Undefined, |(_, value)| value.clone())
    }

    /// Returns whether a key exists.
    #[must_use]
    pub fn has(&self, key: &Value) -> bool {
        self.slot()
            .borrow()
            .iter()
            .flatten()
            .any(|(existing, _)| existing.same_value_zero(key))
    }

    /// Deletes a key without disturbing the order of surviving entries.
    pub fn delete(&self, key: &Value) -> bool {
        let slot = self.slot();
        let mut entries = slot.borrow_mut();
        let Some(index) = entries.iter().position(|candidate| {
            candidate
                .as_ref()
                .is_some_and(|(existing, _)| existing.same_value_zero(key))
        }) else {
            return false;
        };
        entries[index] = None;
        true
    }

    /// Returns the current number of entries.
    #[must_use]
    pub fn size(&self) -> usize {
        self.slot().borrow().iter().flatten().count()
    }

    /// Creates a live iterator that observes later additions.
    #[must_use]
    pub fn entries(&self) -> MapIterator {
        MapIterator {
            entries: self.slot(),
            index: 0,
        }
    }

    fn slot(&self) -> MapSlot {
        self.0.0.state.borrow().map_data.clone().expect("Map slot")
    }
}

impl Default for MapObject {
    fn default() -> Self {
        Self::new()
    }
}

/// A live Map entry iterator.
pub struct MapIterator {
    entries: MapSlot,
    index: usize,
}

impl Iterator for MapIterator {
    type Item = (Value, Value);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let value = self.entries.borrow().get(self.index).cloned();
            self.index += 1;
            match value {
                Some(Some(value)) => return Some(value),
                Some(None) => {}
                None => return None,
            }
        }
    }
}

/// An insertion-ordered Set using SameValueZero equality.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetObject(Object);

impl SetObject {
    /// Creates an empty Set.
    #[must_use]
    pub fn new() -> Self {
        let object = Object::new();
        object.0.state.borrow_mut().set_data = Some(Rc::new(RefCell::new(Vec::new())));
        Self(object)
    }
    /// Wraps an object with Set internal slots.
    #[must_use]
    pub fn from_object(object: Object) -> Option<Self> {
        let present = object.0.state.borrow().set_data.is_some();
        present.then_some(Self(object))
    }
    /// Returns the underlying object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }
    /// Adds a value if absent.
    pub fn add(&self, value: Value) {
        if !self.has(&value) {
            self.slot().borrow_mut().push(Some(normalize_zero(value)));
        }
    }
    /// Returns whether a value exists.
    #[must_use]
    pub fn has(&self, value: &Value) -> bool {
        self.slot()
            .borrow()
            .iter()
            .flatten()
            .any(|item| item.same_value_zero(value))
    }
    /// Deletes a value.
    pub fn delete(&self, value: &Value) -> bool {
        let slot = self.slot();
        let mut entries = slot.borrow_mut();
        let Some(index) = entries.iter().position(|item| {
            item.as_ref()
                .is_some_and(|item| item.same_value_zero(value))
        }) else {
            return false;
        };
        entries[index] = None;
        true
    }
    /// Returns the current size.
    #[must_use]
    pub fn size(&self) -> usize {
        self.slot().borrow().iter().flatten().count()
    }
    /// Creates a live value iterator.
    #[must_use]
    pub fn values(&self) -> SetIterator {
        SetIterator {
            entries: self.slot(),
            index: 0,
        }
    }
    fn slot(&self) -> SetSlot {
        self.0.0.state.borrow().set_data.clone().expect("Set slot")
    }
}

impl Default for SetObject {
    fn default() -> Self {
        Self::new()
    }
}

/// A live Set value iterator.
pub struct SetIterator {
    entries: SetSlot,
    index: usize,
}
impl Iterator for SetIterator {
    type Item = Value;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let value = self.entries.borrow().get(self.index).cloned();
            self.index += 1;
            match value {
                Some(Some(value)) => return Some(value),
                Some(None) => {}
                None => return None,
            }
        }
    }
}

/// A weak-key Map that does not keep Object keys alive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeakMapObject(Object);
impl WeakMapObject {
    /// Creates an empty WeakMap.
    #[must_use]
    pub fn new() -> Self {
        let object = Object::new();
        object.0.state.borrow_mut().weak_map_data = Some(Rc::new(RefCell::new(Vec::new())));
        Self(object)
    }
    /// Returns the underlying object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }
    /// Wraps an object with WeakMap internal slots.
    #[must_use]
    pub fn from_object(object: Object) -> Option<Self> {
        let present = object.0.state.borrow().weak_map_data.is_some();
        present.then_some(Self(object))
    }
    /// Associates a value with an Object key.
    pub fn set(&self, key: &Value, value: Value) -> RuntimeResult<()> {
        let Value::Object(key) = key else {
            return Err(RuntimeError::type_error("WeakMap key must be an object"));
        };
        self.cleanup();
        let slot = self.slot();
        let mut entries = slot.borrow_mut();
        if let Some((_, existing)) = entries
            .iter_mut()
            .find(|(weak, _)| weak.upgrade().as_ref() == Some(key))
        {
            *existing = value;
        } else {
            entries.push((key.downgrade(), value));
        }
        Ok(())
    }
    /// Gets a live key's value.
    #[must_use]
    pub fn get(&self, key: &Object) -> Value {
        self.cleanup();
        self.slot()
            .borrow()
            .iter()
            .find(|(weak, _)| weak.upgrade().as_ref() == Some(key))
            .map_or(Value::Undefined, |(_, value)| value.clone())
    }
    /// Returns the live entry count for diagnostics and GC tests.
    #[must_use]
    pub fn live_len(&self) -> usize {
        self.cleanup();
        self.slot().borrow().len()
    }
    fn cleanup(&self) {
        self.slot()
            .borrow_mut()
            .retain(|(key, _)| key.upgrade().is_some());
    }
    fn slot(&self) -> WeakMapSlot {
        self.0
            .0
            .state
            .borrow()
            .weak_map_data
            .clone()
            .expect("WeakMap slot")
    }
}
impl Default for WeakMapObject {
    fn default() -> Self {
        Self::new()
    }
}

/// A weak-key Set that does not keep Object values alive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeakSetObject(Object);
impl WeakSetObject {
    /// Creates an empty WeakSet.
    #[must_use]
    pub fn new() -> Self {
        let object = Object::new();
        object.0.state.borrow_mut().weak_set_data = Some(Rc::new(RefCell::new(Vec::new())));
        Self(object)
    }
    /// Returns the underlying object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }
    /// Wraps an object with WeakSet internal slots.
    #[must_use]
    pub fn from_object(object: Object) -> Option<Self> {
        let present = object.0.state.borrow().weak_set_data.is_some();
        present.then_some(Self(object))
    }
    /// Adds an Object key.
    pub fn add(&self, value: &Value) -> RuntimeResult<()> {
        let Value::Object(object) = value else {
            return Err(RuntimeError::type_error("WeakSet value must be an object"));
        };
        self.cleanup();
        if !self.has(object) {
            self.slot().borrow_mut().push(object.downgrade());
        }
        Ok(())
    }
    /// Returns whether an Object remains live in the set.
    #[must_use]
    pub fn has(&self, value: &Object) -> bool {
        self.cleanup();
        self.slot()
            .borrow()
            .iter()
            .any(|weak| weak.upgrade().as_ref() == Some(value))
    }
    /// Returns the live key count for diagnostics and GC tests.
    #[must_use]
    pub fn live_len(&self) -> usize {
        self.cleanup();
        self.slot().borrow().len()
    }
    fn cleanup(&self) {
        self.slot()
            .borrow_mut()
            .retain(|key| key.upgrade().is_some());
    }
    fn slot(&self) -> WeakSetSlot {
        self.0
            .0
            .state
            .borrow()
            .weak_set_data
            .clone()
            .expect("WeakSet slot")
    }
}
impl Default for WeakSetObject {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_zero(value: Value) -> Value {
    if let Value::Number(number) = &value {
        if *number == 0.0 {
            return Value::Number(0.0);
        }
    }
    value
}
