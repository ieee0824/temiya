//! Array indices are separate from insertion-ordered named properties.
//! Complete descriptors survive every representation change. No method here
//! invokes JavaScript; callers perform descriptor/prototype/Proxy checks.

use std::{borrow::Cow, collections::BTreeMap};

use super::{PropertyDescriptor, Value};

pub(super) enum Elements {
    /// Contiguous ordinary writable/enumerable/configurable numeric data.
    PackedNumbers(Vec<f64>),
    Packed(Vec<PropertyDescriptor>),
    Holey {
        slots: Vec<Option<PropertyDescriptor>>,
        present: usize,
    },
    Sparse(BTreeMap<u32, PropertyDescriptor>),
}

impl Default for Elements {
    fn default() -> Self {
        Self::Packed(Vec::new())
    }
}

impl Elements {
    pub(super) fn get(&self, index: u32) -> Option<Cow<'_, PropertyDescriptor>> {
        match self {
            Self::PackedNumbers(slots) => slots
                .get(index as usize)
                .map(|&value| Cow::Owned(number_descriptor(value))),
            Self::Packed(slots) => slots.get(index as usize).map(Cow::Borrowed),
            Self::Holey { slots, .. } => slots
                .get(index as usize)
                .and_then(Option::as_ref)
                .map(Cow::Borrowed),
            Self::Sparse(slots) => slots.get(&index).map(Cow::Borrowed),
        }
    }

    pub(super) fn insert(&mut self, index: u32, descriptor: PropertyDescriptor) {
        let position = index as usize;
        if let Self::PackedNumbers(slots) = self {
            if let Some(value) = ordinary_number(&descriptor) {
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
                if let Some(value) = ordinary_number(&descriptor) {
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
                    if slots.last().unwrap().configurable != Some(true) {
                        blocked = Some((slots.len() - 1) as u32);
                        break;
                    }
                    slots.pop();
                }
            }
            Self::Holey { slots, present } => {
                while slots.len() > length as usize {
                    if let Some(descriptor) = slots.last().unwrap() {
                        if descriptor.configurable != Some(true) {
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
                    if descriptor.configurable != Some(true) {
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
                .map(|(index, value)| (index as u32, number_descriptor(value)))
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
                    .map(number_descriptor)
                    .collect(),
            );
        }
    }
}

fn ordinary_number(descriptor: &PropertyDescriptor) -> Option<f64> {
    match descriptor {
        PropertyDescriptor {
            value: Some(Value::Number(value)),
            writable: Some(true),
            get: None,
            set: None,
            enumerable: Some(true),
            configurable: Some(true),
        } => Some(*value),
        _ => None,
    }
}

fn number_descriptor(value: f64) -> PropertyDescriptor {
    PropertyDescriptor::data(Value::Number(value), true, true, true)
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
    fn mixed_transitions_match_ordered_descriptor_model() {
        use std::collections::BTreeMap;
        let mut elements = Elements::default();
        let mut expected = BTreeMap::new();
        for i in 0..64 {
            let descriptor = number_descriptor(f64::from(i));
            elements.insert(i, descriptor.clone());
            expected.insert(i, descriptor);
        }
        let mut state = 0x4d59_5df4_d0f3_3173_u64;
        for step in 0..2000 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let index = if state & 255 == 0 {
                u32::MAX - 1
            } else {
                ((state >> 16) % 180) as u32
            };
            match (state >> 32) % 5 {
                0..=2 => {
                    let value = if state & 8 == 0 {
                        Value::Number(f64::from_bits(state))
                    } else {
                        Value::Undefined
                    };
                    let descriptor = PropertyDescriptor::data(
                        value,
                        state & 16 == 0,
                        state & 32 == 0,
                        state & 64 == 0,
                    );
                    elements.insert(index, descriptor.clone());
                    expected.insert(index, descriptor);
                }
                3 => {
                    if expected
                        .get(&index)
                        .is_none_or(|d| d.configurable == Some(true))
                    {
                        elements.remove(index);
                        expected.remove(&index);
                    }
                }
                _ => {
                    let length = index.min(180);
                    let mut blocked = None;
                    for key in expected
                        .range(length..)
                        .map(|(&key, _)| key)
                        .rev()
                        .collect::<Vec<_>>()
                    {
                        if expected[&key].configurable != Some(true) {
                            blocked = Some(key);
                            break;
                        }
                        expected.remove(&key);
                    }
                    assert_eq!(elements.shrink(length), blocked, "step {step}");
                }
            }
            assert_eq!(
                elements.indices(),
                expected.keys().copied().collect::<Vec<_>>(),
                "step {step}"
            );
            for (&index, value) in &expected {
                let actual = elements.get(index).expect("present descriptor");
                assert!(
                    actual
                        .value
                        .as_ref()
                        .unwrap()
                        .same_value(value.value.as_ref().unwrap()),
                    "step {step}, index {index}"
                );
                assert_eq!(
                    (actual.writable, actual.enumerable, actual.configurable),
                    (value.writable, value.enumerable, value.configurable),
                    "step {step}, index {index}"
                );
                assert!(actual.get.is_none() && actual.set.is_none());
            }
        }
    }

    #[test]
    fn numeric_packing_preserves_bits_and_expands_for_holes_and_attributes() {
        let mut elements = Elements::default();
        let nan = f64::from_bits(0x7ff8_1234_5678_9abc);
        for (index, value) in [0.0, -0.0, nan, f64::INFINITY].into_iter().enumerate() {
            elements.insert(index as u32, number_descriptor(value));
        }
        assert!(matches!(&elements, Elements::PackedNumbers(slots) if slots.len() == 4));
        for (index, value) in [0.0, -0.0, nan, f64::INFINITY].into_iter().enumerate() {
            let descriptor = elements.get(index as u32).unwrap();
            let Some(Value::Number(actual)) = &descriptor.value else {
                panic!("number expected")
            };
            assert_eq!(actual.to_bits(), value.to_bits());
            assert_eq!(
                (
                    descriptor.writable,
                    descriptor.enumerable,
                    descriptor.configurable
                ),
                (Some(true), Some(true), Some(true))
            );
        }
        elements.remove(1);
        assert!(matches!(elements, Elements::Holey { .. }));
        assert!(elements.get(1).is_none());
        elements.insert(1, number_descriptor(42.0));
        let frozen = PropertyDescriptor::data(Value::Number(7.0), false, true, false);
        elements.insert(0, frozen);
        assert_eq!(elements.shrink(0), Some(0));
        let first = elements.get(0).unwrap();
        assert_eq!(
            (first.writable, first.configurable),
            (Some(false), Some(false))
        );
    }

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
