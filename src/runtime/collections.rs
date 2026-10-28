//! Insertion-ordered and weak-keyed collection objects.

use std::{cell::RefCell, rc::Rc};

use super::{
    Object, RuntimeError, RuntimeResult, Value, WeakObject,
    stored::{StoredObject, StoredValue},
};

type MapEntries = Vec<Option<(StoredValue, StoredValue)>>;
type SetEntries = Vec<Option<StoredValue>>;
pub(super) type MapSlot = Rc<RefCell<MapEntries>>;
pub(super) type SetSlot = Rc<RefCell<SetEntries>>;
pub(super) type WeakMapSlot = Rc<RefCell<Vec<(WeakObject, StoredValue)>>>;
pub(super) type WeakSetSlot = Rc<RefCell<Vec<WeakObject>>>;

/// An insertion-ordered Map using SameValueZero key equality.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapObject(Object);

impl MapObject {
    /// Creates an empty Map.
    #[must_use]
    pub fn new(context: &super::AllocationContext) -> Self {
        Self::from_empty(Object::new_in(context))
    }

    pub(super) fn new_for(context: &Object) -> Self {
        Self::from_empty(Object::new_for(context))
    }

    fn from_empty(object: Object) -> Self {
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
    ///
    /// # Panics
    /// Panics if an Object key or value belongs to another allocation context.
    /// Use `try_set` to handle that error.
    pub fn set(&self, key: Value, value: Value) {
        self.try_set(key, value)
            .expect("Map entries must share their allocation context");
    }

    /// Inserts a key and value, rejecting foreign allocation contexts before mutation.
    pub fn try_set(&self, key: Value, value: Value) -> RuntimeResult<()> {
        for item in [&key, &value] {
            if let Value::Object(object) = item {
                StoredObject::check_context(&self.0, object)?;
            }
        }
        let value = StoredValue::store_owned(&self.0, value)?;
        let slot = self.slot();
        let mut entries = slot.borrow_mut();
        if let Some((_, existing)) = entries
            .iter_mut()
            .flatten()
            .find(|(existing, _)| existing.same_value_zero(&key))
        {
            *existing = value;
        } else {
            entries.push(Some((
                StoredValue::store_owned(&self.0, normalize_zero(key))?,
                value,
            )));
        }
        Ok(())
    }

    /// Returns a value for a key, or `undefined` when absent.
    #[must_use]
    pub fn get(&self, key: &Value) -> Value {
        self.slot()
            .borrow()
            .iter()
            .flatten()
            .find(|(existing, _)| existing.same_value_zero(key))
            .map_or(Value::Undefined, |(_, value)| {
                value.load(&self.0).expect("live Map entry")
            })
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
            owner: self.0.clone(),
            index: 0,
        }
    }

    fn slot(&self) -> MapSlot {
        self.0.0.state.borrow().map_data.clone().expect("Map slot")
    }
}

/// A live Map entry iterator.
pub struct MapIterator {
    owner: Object,
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
                Some(Some((key, value))) => {
                    return Some((
                        key.load(&self.owner).expect("live Map key"),
                        value.load(&self.owner).expect("live Map value"),
                    ));
                }
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
    pub fn new(context: &super::AllocationContext) -> Self {
        Self::from_empty(Object::new_in(context))
    }

    pub(super) fn new_for(context: &Object) -> Self {
        Self::from_empty(Object::new_for(context))
    }

    fn from_empty(object: Object) -> Self {
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
    ///
    /// # Panics
    /// Panics if an Object value belongs to another allocation context.
    /// Use `try_add` to handle that error.
    pub fn add(&self, value: Value) {
        self.try_add(value)
            .expect("Set entries must share their allocation context");
    }

    /// Adds a value, rejecting a foreign allocation context before mutation.
    pub fn try_add(&self, value: Value) -> RuntimeResult<()> {
        if let Value::Object(object) = &value {
            StoredObject::check_context(&self.0, object)?;
        }
        if !self.has(&value) {
            self.slot().borrow_mut().push(Some(StoredValue::store_owned(
                &self.0,
                normalize_zero(value),
            )?));
        }
        Ok(())
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
            owner: self.0.clone(),
            index: 0,
        }
    }
    fn slot(&self) -> SetSlot {
        self.0.0.state.borrow().set_data.clone().expect("Set slot")
    }
}

/// A live Set value iterator.
pub struct SetIterator {
    owner: Object,
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
                Some(Some(value)) => return Some(value.load(&self.owner).expect("live Set value")),
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
    pub fn new(context: &super::AllocationContext) -> Self {
        Self::from_empty(Object::new_in(context))
    }
    pub(super) fn new_for(context: &Object) -> Self {
        Self::from_empty(Object::new_for(context))
    }
    fn from_empty(object: Object) -> Self {
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
        StoredObject::check_context(&self.0, key)?;
        if let Value::Object(object) = &value {
            StoredObject::check_context(&self.0, object)?;
        }
        let _ = StoredObject::store(&self.0, key)?;
        let value = StoredValue::store_owned(&self.0, value)?;
        self.cleanup();
        let slot = self.slot();
        let mut entries = slot.borrow_mut();
        if let Some((_, existing)) = entries.iter_mut().find(|(weak, _)| weak.0.matches(key)) {
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
            .find(|(weak, _)| weak.0.matches(key))
            .map_or(Value::Undefined, |(_, value)| {
                value.load(&self.0).expect("live WeakMap value")
            })
    }
    /// Returns the live entry count for diagnostics and GC tests.
    #[must_use]
    pub fn live_len(&self) -> usize {
        self.cleanup();
        self.slot().borrow().len()
    }
    fn cleanup(&self) {
        self.slot().borrow_mut().retain(|(key, _)| key.0.is_live());
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
/// A weak-key Set that does not keep Object values alive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeakSetObject(Object);
impl WeakSetObject {
    /// Creates an empty WeakSet.
    #[must_use]
    pub fn new(context: &super::AllocationContext) -> Self {
        Self::from_empty(Object::new_in(context))
    }
    pub(super) fn new_for(context: &Object) -> Self {
        Self::from_empty(Object::new_for(context))
    }
    fn from_empty(object: Object) -> Self {
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
        StoredObject::check_context(&self.0, object)?;
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
            .any(|weak| weak.0.matches(value))
    }
    /// Returns the live key count for diagnostics and GC tests.
    #[must_use]
    pub fn live_len(&self) -> usize {
        self.cleanup();
        self.slot().borrow().len()
    }
    fn cleanup(&self) {
        self.slot().borrow_mut().retain(|key| key.0.is_live());
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
fn normalize_zero(value: Value) -> Value {
    if let Value::Number(number) = &value {
        if *number == 0.0 {
            return Value::Number(0.0);
        }
    }
    value
}

#[cfg(test)]
mod managed_tests {
    use super::*;
    use crate::runtime::Runtime;

    #[test]
    fn managed_weak_maps_require_both_owner_and_key_without_bootstrapping_cycles() {
        for drop_owner in [true, false] {
            let runtime = Runtime::new();
            let context = Object::new_in(&runtime);
            let map = WeakMapObject::new_for(&context);
            drop(context);
            let key = Object::new_in(&runtime);
            runtime.collect_migration_graph().unwrap();
            let value = Object::new_in(&runtime);
            value
                .create_data_property("key".into(), Value::Object(key.clone()))
                .unwrap();
            map.set(&Value::Object(key.clone()), Value::Object(value.clone()))
                .unwrap();
            runtime.verify_migration_barriers().unwrap();
            let weak = [key.downgrade(), value.downgrade()];
            drop(value);
            assert_eq!(runtime.diagnostics().rooted, 2);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 3);
            assert!(matches!(map.get(&key), Value::Object(_)));
            if drop_owner {
                drop(map);
                assert_eq!(runtime.collect_migration_graph().unwrap().retained, 1);
                assert!(weak[0].upgrade().is_some());
                assert!(weak[1].upgrade().is_none());
                drop(key);
            } else {
                drop(key);
                assert_eq!(runtime.collect_migration_graph().unwrap().retained, 1);
                assert!(weak.iter().all(|weak| weak.upgrade().is_none()));
                assert_eq!(map.live_len(), 0);
                drop(map);
            }
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        }
    }

    #[test]
    fn managed_weak_map_chains_reach_a_fixed_point_and_weak_sets_do_not_root_keys() {
        let runtime = Runtime::new();
        let context = Object::new_in(&runtime);
        let first = WeakMapObject::new_for(&context);
        let second = WeakMapObject::new_for(&context);
        let set = WeakSetObject::new_for(&context);
        drop(context);
        let key = Object::new_in(&runtime);
        let next_key = Object::new_in(&runtime);
        let value = Object::new_in(&runtime);
        value
            .create_data_property("key".into(), Value::Object(key.clone()))
            .unwrap();
        value
            .create_data_property("answer".into(), Value::Number(42.0))
            .unwrap();
        first
            .set(&Value::Object(key.clone()), Value::Object(next_key.clone()))
            .unwrap();
        second
            .set(
                &Value::Object(next_key.clone()),
                Value::Object(value.clone()),
            )
            .unwrap();
        set.add(&Value::Object(next_key.clone())).unwrap();
        drop((next_key, value));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 6);
        let Value::Object(next_key) = first.get(&key) else {
            panic!("live next key")
        };
        assert!(set.has(&next_key));
        let Value::Object(value) = second.get(&next_key) else {
            panic!("live value")
        };
        assert!(
            value
                .get(&"answer".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop((key, next_key, value));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 3);
        assert_eq!(first.live_len(), 0);
        assert_eq!(second.live_len(), 0);
        assert_eq!(set.live_len(), 0);
        let foreign_runtime = Runtime::new();
        let foreign = Object::new_in(&foreign_runtime);
        let key = Object::new_in(&runtime);
        first
            .set(&Value::Object(key.clone()), Value::Number(42.0))
            .unwrap();
        assert!(
            first
                .set(&Value::Object(key.clone()), Value::Object(foreign.clone()))
                .is_err()
        );
        assert!(
            first
                .set(&Value::Object(foreign.clone()), Value::Number(1.0))
                .is_err()
        );
        assert!(set.add(&Value::Object(foreign)).is_err());
        assert!(first.get(&key).same_value(&Value::Number(42.0)));
        assert_eq!(first.live_len(), 1);
        assert_eq!(set.live_len(), 0);
        drop((first, second, set, key));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }

    #[test]
    fn managed_collection_cycles_remain_live_through_public_iterators() {
        let runtime = Runtime::new();
        let context = Object::new_in(&runtime);
        let map = MapObject::new_for(&context);
        let set = SetObject::new_for(&context);
        drop(context);
        runtime.collect_migration_graph().unwrap();
        let key = Object::new_in(&runtime);
        let value = Object::new_in(&runtime);
        value
            .create_data_property("answer".into(), Value::Number(42.0))
            .unwrap();
        key.create_data_property("map".into(), Value::Object(map.object()))
            .unwrap();
        value
            .create_data_property("set".into(), Value::Object(set.object()))
            .unwrap();
        map.try_set(Value::Object(key.clone()), Value::Object(value.clone()))
            .unwrap();
        set.try_add(Value::Object(map.object())).unwrap();
        let weak = [
            map.object().downgrade(),
            set.object().downgrade(),
            key.downgrade(),
            value.downgrade(),
        ];
        runtime.verify_migration_barriers().unwrap();
        let mut entries = map.entries();
        let mut values = set.values();
        drop(key);
        drop(value);
        drop(map);
        drop(set);
        assert_eq!(runtime.diagnostics().rooted, 2);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 4);
        let (key, value) = entries.next().unwrap();
        let Value::Object(value) = value else {
            panic!("object value")
        };
        assert!(
            value
                .get(&"answer".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        let Value::Object(map) = values.next().unwrap() else {
            panic!("map value")
        };
        let map = MapObject::from_object(map).unwrap();
        assert!(map.has(&key));
        map.try_set(Value::Number(1.0), Value::Number(42.0))
            .unwrap();
        assert!(entries.next().unwrap().1.same_value(&Value::Number(42.0)));
        drop((key, value, map, entries));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 4);
        drop(values);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.iter().all(|weak| weak.upgrade().is_none()));
    }

    #[test]
    fn managed_collection_foreign_writes_are_atomic_and_keys_need_no_facades() {
        let runtime = Runtime::new();
        let context = Object::new_in(&runtime);
        let map = MapObject::new_for(&context);
        let set = SetObject::new_for(&context);
        let key = Object::new_in(&runtime);
        map.try_set(Value::Object(key.clone()), Value::Number(42.0))
            .unwrap();
        let foreign = Object::new();
        assert!(
            map.try_set(Value::Object(key.clone()), Value::Object(foreign.clone()))
                .is_err()
        );
        assert!(
            map.get(&Value::Object(key.clone()))
                .same_value(&Value::Number(42.0))
        );
        assert!(
            map.try_set(Value::Object(foreign.clone()), Value::Number(1.0))
                .is_err()
        );
        assert!(set.try_add(Value::Object(foreign)).is_err());
        assert_eq!(map.size(), 1);
        assert_eq!(set.size(), 0);
        map.try_set(Value::Number(f64::NAN), Value::Number(1.0))
            .unwrap();
        map.try_set(Value::Number(f64::NAN), Value::Number(42.0))
            .unwrap();
        map.try_set(Value::Number(-0.0), Value::Number(42.0))
            .unwrap();
        assert!(
            map.get(&Value::Number(0.0))
                .same_value(&Value::Number(42.0))
        );
        assert_eq!(map.size(), 3);
        let before = runtime.diagnostics().rooted;
        for _ in 0..100 {
            assert!(map.has(&Value::Number(f64::NAN)));
            assert!(!map.has(&Value::Number(99.0)));
        }
        assert_eq!(runtime.diagnostics().rooted, before);
        assert!(!map.has(&Value::Object(Object::new())));
        assert!(map.delete(&Value::Number(f64::NAN)));
        set.try_add(Value::Number(-0.0)).unwrap();
        set.try_add(Value::Number(0.0)).unwrap();
        let Value::Number(zero) = set.values().next().unwrap() else {
            panic!("number")
        };
        assert!(!zero.is_sign_negative());
        assert_eq!(set.size(), 1);
        assert!(set.delete(&Value::Number(-0.0)));
    }
}
