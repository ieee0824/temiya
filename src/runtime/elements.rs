//! Array indices are separate from insertion-ordered named properties.
//! Complete descriptors survive every representation change. No method here
//! invokes JavaScript; callers perform descriptor/prototype/Proxy checks.

use std::{borrow::Cow, collections::BTreeMap};

use super::PropertyDescriptor;

pub(super) trait ElementDescriptor: Clone {
    fn configurable(&self) -> Option<bool>;
    /// Only complete, default-attribute numeric data descriptors qualify.
    fn ordinary_number(&self) -> Option<f64>;
    fn from_number(value: f64) -> Self;
}
impl ElementDescriptor for PropertyDescriptor {
    fn configurable(&self) -> Option<bool> {
        self.configurable
    }
    fn ordinary_number(&self) -> Option<f64> {
        match self {
            Self {
                value: Some(super::Value::Number(value)),
                writable: Some(true),
                enumerable: Some(true),
                configurable: Some(true),
                get: None,
                set: None,
            } => Some(*value),
            _ => None,
        }
    }
    fn from_number(value: f64) -> Self {
        Self::data(super::Value::Number(value), true, true, true)
    }
}
impl ElementDescriptor for super::stored::StoredDescriptor {
    fn configurable(&self) -> Option<bool> {
        self.configurable()
    }
    fn ordinary_number(&self) -> Option<f64> {
        match self {
            Self::Data {
                value: super::stored::StoredValue::Number(value),
                writable: true,
                enumerable: true,
                configurable: true,
            } => Some(*value),
            _ => None,
        }
    }
    fn from_number(value: f64) -> Self {
        Self::Data {
            value: super::stored::StoredValue::Number(value),
            writable: true,
            enumerable: true,
            configurable: true,
        }
    }
}

pub(super) enum Elements<D: ElementDescriptor = PropertyDescriptor> {
    /// No holes, accessors, nondefault flags, or managed edges.
    PackedNumbers(Vec<f64>),
    Packed(Vec<D>),
    Holey {
        slots: Vec<Option<D>>,
        present: usize,
    },
    Sparse(BTreeMap<u32, D>),
}

impl<D: ElementDescriptor> Default for Elements<D> {
    fn default() -> Self {
        Self::Packed(Vec::new())
    }
}

impl<D: ElementDescriptor> Elements<D> {
    pub(super) fn for_each_traceable(&self, mut visit: impl FnMut(&D)) {
        match self {
            // Synthesized Number descriptors contain no managed handles.
            Self::PackedNumbers(_) => {}
            Self::Packed(values) => values.iter().for_each(visit),
            Self::Holey { slots, .. } => slots.iter().flatten().for_each(visit),
            Self::Sparse(values) => values.values().for_each(&mut visit),
        }
    }
    pub(super) fn is_empty(&self) -> bool {
        match self {
            Self::PackedNumbers(slots) => slots.is_empty(),
            Self::Packed(slots) => slots.is_empty(),
            Self::Holey { present, .. } => *present == 0,
            Self::Sparse(slots) => slots.is_empty(),
        }
    }

    pub(super) fn get(&self, index: u32) -> Option<Cow<'_, D>> {
        match self {
            Self::PackedNumbers(slots) => slots
                .get(index as usize)
                .map(|value| Cow::Owned(D::from_number(*value))),
            Self::Packed(slots) => slots.get(index as usize).map(Cow::Borrowed),
            Self::Holey { slots, .. } => slots
                .get(index as usize)
                .and_then(Option::as_ref)
                .map(Cow::Borrowed),
            Self::Sparse(slots) => slots.get(&index).map(Cow::Borrowed),
        }
    }

    pub(super) fn insert(&mut self, index: u32, descriptor: D) {
        let position = index as usize;
        if let Self::PackedNumbers(slots) = self {
            if let Some(value) = descriptor.ordinary_number() {
                if position < slots.len() {
                    slots[position] = value;
                    return;
                }
                if position == slots.len() {
                    slots.push(value);
                    return;
                }
            }
            self.expand_numbers();
        }
        if let Self::Packed(slots) = self {
            if slots.is_empty() && position == 0 {
                if let Some(value) = descriptor.ordinary_number() {
                    *self = Self::PackedNumbers(vec![value]);
                    return;
                }
            }
        }
        match self {
            Self::PackedNumbers(_) => unreachable!(),
            Self::Packed(slots) if position < slots.len() => slots[position] = descriptor,
            Self::Packed(slots) if position == slots.len() => slots.push(descriptor),
            Self::Packed(slots) if dense_enough(position + 1, slots.len() + 1) => {
                let present = slots.len() + 1;
                let mut slots: Vec<_> = std::mem::take(slots).into_iter().map(Some).collect();
                slots.resize_with(position + 1, || None);
                slots[position] = Some(descriptor);
                *self = Self::Holey { slots, present };
            }
            Self::Holey { slots, present }
                if position < slots.len() || dense_enough(position + 1, *present + 1) =>
            {
                if position >= slots.len() {
                    slots.resize_with(position + 1, || None);
                }
                if slots[position].replace(descriptor).is_none() {
                    *present += 1;
                }
                self.compact();
            }
            Self::Sparse(slots) => {
                slots.insert(index, descriptor);
                self.compact();
            }
            _ => {
                self.make_sparse();
                let Self::Sparse(slots) = self else {
                    unreachable!()
                };
                slots.insert(index, descriptor);
            }
        }
    }

    /// Delete an already-validated configurable property (or a missing index).
    pub(super) fn remove(&mut self, index: u32) {
        let position = index as usize;
        if let Self::PackedNumbers(slots) = self {
            if position >= slots.len() {
                return;
            }
            if position + 1 == slots.len() {
                slots.pop();
                self.compact();
                return;
            }
            self.expand_numbers();
        }
        match self {
            Self::PackedNumbers(_) => unreachable!(),
            Self::Packed(slots) if position >= slots.len() => return,
            Self::Packed(slots) if position + 1 == slots.len() => {
                slots.pop();
            }
            Self::Packed(slots) => {
                let present = slots.len() - 1;
                let mut slots: Vec<_> = std::mem::take(slots).into_iter().map(Some).collect();
                slots[position] = None;
                *self = Self::Holey { slots, present };
            }
            Self::Holey { slots, present } => {
                if let Some(slot) = slots.get_mut(position) {
                    *present -= usize::from(slot.take().is_some());
                }
            }
            Self::Sparse(slots) => {
                slots.remove(&index);
            }
        }
        self.compact();
    }

    /// Delete existing indices in descending order, stopping at the first
    /// nonconfigurable property. Work depends on stored slots, not Array length.
    pub(super) fn shrink(&mut self, length: u32) -> Option<u32> {
        let mut blocked = None;
        match self {
            Self::PackedNumbers(slots) => slots.truncate(length as usize),
            Self::Packed(slots) => {
                while slots.len() > length as usize {
                    if slots.last().unwrap().configurable() != Some(true) {
                        blocked = Some((slots.len() - 1) as u32);
                        break;
                    }
                    slots.pop();
                }
            }
            Self::Holey { slots, present } => {
                while slots.len() > length as usize {
                    if let Some(descriptor) = slots.last().unwrap() {
                        if descriptor.configurable() != Some(true) {
                            blocked = Some((slots.len() - 1) as u32);
                            break;
                        }
                        *present -= 1;
                    }
                    slots.pop();
                }
            }
            Self::Sparse(slots) => {
                while let Some((&index, descriptor)) = slots.last_key_value() {
                    if index < length {
                        break;
                    }
                    if descriptor.configurable() != Some(true) {
                        blocked = Some(index);
                        break;
                    }
                    slots.pop_last();
                }
            }
        }
        self.compact();
        blocked
    }

    pub(super) fn indices(&self) -> Vec<u32> {
        match self {
            Self::PackedNumbers(slots) => (0..slots.len() as u32).collect(),
            Self::Packed(slots) => (0..slots.len() as u32).collect(),
            Self::Holey { slots, .. } => slots
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| slot.as_ref().map(|_| index as u32))
                .collect(),
            Self::Sparse(slots) => slots.keys().copied().collect(),
        }
    }

    fn make_sparse(&mut self) {
        let slots = match std::mem::take(self) {
            Self::PackedNumbers(slots) => slots
                .into_iter()
                .enumerate()
                .map(|(index, value)| (index as u32, D::from_number(value)))
                .collect(),
            Self::Packed(slots) => slots
                .into_iter()
                .enumerate()
                .map(|(index, descriptor)| (index as u32, descriptor))
                .collect(),
            Self::Holey { slots, .. } => slots
                .into_iter()
                .enumerate()
                .filter_map(|(index, descriptor)| descriptor.map(|d| (index as u32, d)))
                .collect(),
            Self::Sparse(slots) => slots,
        };
        *self = Self::Sparse(slots);
    }

    fn compact(&mut self) {
        match self {
            Self::PackedNumbers(slots) => release_excess(slots),
            Self::Packed(slots) => release_excess(slots),
            Self::Holey { slots, present } => {
                while slots.last().is_some_and(Option::is_none) {
                    slots.pop();
                }
                if *present == slots.len() {
                    *self = Self::Packed(std::mem::take(slots).into_iter().flatten().collect());
                } else if slots.len() > 64 && *present < slots.len() / 4 {
                    self.make_sparse();
                } else {
                    release_excess(slots);
                }
            }
            Self::Sparse(slots) => {
                let span = slots
                    .last_key_value()
                    .map_or(0, |(&index, _)| index as usize + 1);
                // Hysteresis: become dense at >= 50%, sparse below 25%.
                if !dense_enough(span, slots.len()) {
                    return;
                }
                let present = slots.len();
                if span == present {
                    *self = Self::Packed(std::mem::take(slots).into_values().collect());
                } else {
                    let mut dense = Vec::with_capacity(span);
                    dense.resize_with(span, || None);
                    for (index, descriptor) in std::mem::take(slots) {
                        dense[index as usize] = Some(descriptor);
                    }
                    *self = Self::Holey {
                        slots: dense,
                        present,
                    };
                }
            }
        }
    }

    fn expand_numbers(&mut self) {
        if let Self::PackedNumbers(slots) = self {
            *self = Self::Packed(
                std::mem::take(slots)
                    .into_iter()
                    .map(D::from_number)
                    .collect(),
            );
        }
    }
}

fn dense_enough(span: usize, present: usize) -> bool {
    span <= 64 || present >= span.div_ceil(2)
}

fn release_excess<T>(slots: &mut Vec<T>) {
    // Retain a small buffer for repeated push/clear without keeping arbitrarily
    // large allocations alive after length shrink or repeated pop.
    if slots.capacity() > 1024 && slots.len() < slots.capacity() / 4 {
        slots.shrink_to(slots.len().max(64));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Value;

    #[test]
    fn representation_changes_and_storage_stays_bounded_after_clear() {
        let descriptor = PropertyDescriptor::data(Value::Undefined, true, true, true);
        let mut elements = Elements::default();
        for index in 0..4096 {
            elements.insert(index, descriptor.clone());
        }
        assert!(matches!(elements, Elements::Packed(_)));
        elements.remove(17);
        assert!(matches!(elements, Elements::Holey { .. }));
        elements.insert(u32::MAX - 1, descriptor.clone());
        assert!(matches!(&elements, Elements::Sparse(slots) if slots.len() == 4096));
        elements.remove(u32::MAX - 1);
        assert!(matches!(elements, Elements::Holey { .. }));
        elements.insert(17, descriptor);
        assert!(matches!(elements, Elements::Packed(_)));
        assert_eq!(elements.shrink(0), None);
        assert!(
            matches!(&elements, Elements::Packed(slots) if slots.is_empty() && slots.capacity() <= 64)
        );
    }
}
