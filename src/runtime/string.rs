//! Immutable UTF-16 storage with inline names and bounded, lazily flattened ropes.
use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    sync::{Arc, OnceLock},
};

// Fit inline units and tags in the former Vec footprint on 32/64 bit targets.
const INLINE_CAPACITY: usize = (std::mem::size_of::<Vec<u16>>() - 4) / 2;
const ROPE_MIN_LENGTH: usize = 256;
const MAX_ROPE_DEPTH: u8 = 32;

struct Rope {
    left: JsString,
    right: JsString,
    len: usize,
    depth: u8,
    flat: OnceLock<Vec<u16>>,
}

impl Rope {
    // Keep cache initialization out of the hot inline-string access path.
    #[inline(never)]
    fn code_units(&self) -> &[u16] {
        self.flat.get_or_init(|| {
            let mut units = Vec::with_capacity(self.len);
            self.left.append_to(&mut units);
            self.right.append_to(&mut units);
            units
        })
    }
}

#[derive(Clone)]
enum Storage {
    Inline {
        len: u8,
        units: [u16; INLINE_CAPACITY],
    },
    // Preserve the Vec's allocation instead of copying the whole result into
    // a new Arc<[u16]> allocation. All access to this buffer is immutable.
    Shared(Arc<Vec<u16>>),
    Rope(Arc<Rope>),
}

/// A JavaScript String represented as an exact, immutable sequence of UTF-16 code units.
#[derive(Clone)]
pub struct JsString(Storage);

impl JsString {
    // Mutation is private to a binding update. Shared snapshots remain immutable;
    // only Arc::get_mut permits reusing an existing UTF-16 buffer.
    pub(crate) fn append_owned(&mut self, right: &Self) {
        if right.is_empty() {
            return;
        }
        let len = self
            .len()
            .checked_add(right.len())
            .expect("string length overflow");
        if let Storage::Shared(units) = &mut self.0 {
            if let Some(units) = Arc::get_mut(units) {
                units.extend_from_slice(right.code_units());
                return;
            }
        }
        if matches!(&self.0, Storage::Rope(rope) if Arc::strong_count(rope) == 1) {
            let mut units = Vec::with_capacity(len);
            self.append_to(&mut units);
            right.append_to(&mut units);
            self.0 = Storage::Shared(Arc::new(units));
            return;
        }
        *self = self.clone().concat(right.clone());
    }

    /// Concatenates exact UTF-16 sequences, sharing long operands until needed.
    #[must_use]
    pub fn concat(self, right: Self) -> Self {
        if self.is_empty() {
            return right;
        }
        if right.is_empty() {
            return self;
        }
        let len = self
            .len()
            .checked_add(right.len())
            .expect("string length overflow");
        if len <= INLINE_CAPACITY {
            let mut units = [0; INLINE_CAPACITY];
            let split = self.len();
            units[..split].copy_from_slice(self.code_units());
            units[split..len].copy_from_slice(right.code_units());
            return Self(Storage::Inline {
                len: len as u8,
                units,
            });
        }
        let depth = self.depth().max(right.depth()) + 1;
        if len >= ROPE_MIN_LENGTH && depth <= MAX_ROPE_DEPTH {
            return Self(Storage::Rope(Arc::new(Rope {
                left: self,
                right,
                len,
                depth,
                flat: OnceLock::new(),
            })));
        }
        let mut units = Vec::with_capacity(len);
        self.append_to(&mut units);
        right.append_to(&mut units);
        Self::from_code_units(units)
    }

    /// Returns the number of UTF-16 code units without flattening concatenations.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        match &self.0 {
            Storage::Inline { len, .. } => usize::from(*len),
            Storage::Shared(units) => units.len(),
            Storage::Rope(rope) => rope.len,
        }
    }

    fn depth(&self) -> u8 {
        match &self.0 {
            Storage::Rope(rope) => rope.depth,
            _ => 0,
        }
    }

    // Traverse uncached children directly: filling intermediate caches would
    // retain multiple overlapping copies of a long concatenation chain.
    fn append_to(&self, output: &mut Vec<u16>) {
        if let Storage::Rope(rope) = &self.0 {
            if let Some(flat) = rope.flat.get() {
                output.extend_from_slice(flat);
            } else {
                rope.left.append_to(output);
                rope.right.append_to(output);
            }
        } else {
            output.extend_from_slice(self.code_units());
        }
    }

    /// Creates a string from UTF-16 code units without validating surrogate pairs.
    #[must_use]
    pub fn from_code_units(code_units: impl Into<Vec<u16>>) -> Self {
        let units = code_units.into();
        if units.len() <= INLINE_CAPACITY {
            Self::from_code_units_slice(&units)
        } else {
            Self(Storage::Shared(Arc::new(units)))
        }
    }

    /// Copies an exact UTF-16 slice, without a heap allocation for short strings.
    #[must_use]
    pub fn from_code_units_slice(units: &[u16]) -> Self {
        if units.len() <= INLINE_CAPACITY {
            let mut inline = [0; INLINE_CAPACITY];
            inline[..units.len()].copy_from_slice(units);
            Self(Storage::Inline {
                len: units.len() as u8,
                units: inline,
            })
        } else {
            Self(Storage::Shared(Arc::new(units.to_vec())))
        }
    }

    /// Returns the string's UTF-16 code units.
    #[must_use]
    #[inline]
    pub fn code_units(&self) -> &[u16] {
        match &self.0 {
            Storage::Inline { len, units } => &units[..usize::from(*len)],
            Storage::Shared(units) => units,
            Storage::Rope(rope) => rope.code_units(),
        }
    }

    /// Converts the string for diagnostics, replacing unpaired surrogates.
    #[must_use]
    pub fn to_string_lossy(&self) -> String {
        String::from_utf16_lossy(self.code_units())
    }

    /// Returns whether the string contains no UTF-16 code units.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for JsString {
    fn default() -> Self {
        Self(Storage::Inline {
            len: 0,
            units: [0; INLINE_CAPACITY],
        })
    }
}
impl From<&str> for JsString {
    // Keep the ubiquitous short-name constructor small enough to fold literal
    // property keys at call sites. UTF-16 decoding and allocation stay outlined.
    #[inline(always)]
    fn from(value: &str) -> Self {
        if value.len() <= INLINE_CAPACITY && value.is_ascii() {
            let mut units = [0; INLINE_CAPACITY];
            for (slot, byte) in units.iter_mut().zip(value.bytes()) {
                *slot = u16::from(byte);
            }
            Self(Storage::Inline {
                len: value.len() as u8,
                units,
            })
        } else {
            Self::from_long_or_unicode(value)
        }
    }
}
impl JsString {
    #[inline(never)]
    fn from_long_or_unicode(value: &str) -> Self {
        if value.len() > INLINE_CAPACITY && value.is_ascii() {
            // ASCII has exactly one UTF-16 unit per byte.
            return Self(Storage::Shared(Arc::new(
                value.bytes().map(u16::from).collect(),
            )));
        }
        let mut units = [0; INLINE_CAPACITY];
        let mut encoded = value.encode_utf16();
        for (index, slot) in units.iter_mut().enumerate() {
            let Some(unit) = encoded.next() else {
                return Self(Storage::Inline {
                    len: index as u8,
                    units,
                });
            };
            *slot = unit;
        }
        let Some(next) = encoded.next() else {
            return Self(Storage::Inline {
                len: INLINE_CAPACITY as u8,
                units,
            });
        };
        let mut result = Vec::with_capacity(value.len());
        result.extend_from_slice(&units);
        result.push(next);
        result.extend(encoded);
        Self(Storage::Shared(Arc::new(result)))
    }
}
impl From<String> for JsString {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}
impl PartialEq for JsString {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            // Inline buffers are zero-padded by every constructor. Compare them
            // directly, without dispatching through lengths and slices again.
            (
                Storage::Inline {
                    len: a,
                    units: left,
                },
                Storage::Inline {
                    len: b,
                    units: right,
                },
            ) => a == b && left == right,
            _ => self.eq_other(other),
        }
    }
}
impl JsString {
    // Keep the common inline-key comparison small enough to embed at call sites.
    // Shared/mixed storage retains the same identity and exact UTF-16 checks.
    #[inline(never)]
    fn eq_other(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Storage::Shared(left), Storage::Shared(right)) if Arc::ptr_eq(left, right) => {
                return true;
            }
            (Storage::Rope(left), Storage::Rope(right)) if Arc::ptr_eq(left, right) => return true,
            _ => {}
        }
        self.code_units() == other.code_units()
    }
}
impl Eq for JsString {}
impl PartialOrd for JsString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for JsString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.code_units().cmp(other.code_units())
    }
}
impl Hash for JsString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.code_units().hash(state);
    }
}
impl fmt::Debug for JsString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("JsString")
            .field(&self.code_units())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ascii_and_unicode_construction_preserves_inline_boundaries_and_shared_snapshots() {
        use super::*;
        for length in [
            INLINE_CAPACITY - 1,
            INLINE_CAPACITY,
            INLINE_CAPACITY + 1,
            1024,
        ] {
            for text in ["a".repeat(length), format!("{}😀", "a".repeat(length))] {
                let mut value = JsString::from(text.as_str());
                let saved = value.clone();
                value.append_owned(&JsString::from("z"));
                assert_eq!(saved.code_units(), text.encode_utf16().collect::<Vec<_>>());
                assert_eq!(
                    value.code_units(),
                    (text + "z").encode_utf16().collect::<Vec<_>>()
                );
            }
        }
    }
    use super::*;

    #[test]
    fn owned_append_preserves_shared_rope_snapshots_and_utf16() {
        let mut current = JsString::from_code_units(vec![0xd800; 300])
            .concat(JsString::from_code_units(vec![0xdc00; 300]));
        let snapshot = current.clone();
        let previous = snapshot.code_units().to_vec();
        for _ in 0..100 {
            current.append_owned(&JsString::from_code_units_slice(&[0xdfff, 97]));
        }
        assert_eq!(snapshot.code_units(), previous.as_slice());
        assert_eq!(&current.code_units()[..600], previous.as_slice());
        assert_eq!(current.len(), 800);
        assert_eq!(&current.code_units()[798..], &[0xdfff, 97]);
    }

    #[test]
    fn lengths_and_shared_equality_do_not_flatten_rope_children() {
        let leaf = JsString::from("x".repeat(ROPE_MIN_LENGTH));
        let first = leaf.clone().concat(JsString::from("y"));
        let Storage::Rope(child) = &first.0 else {
            panic!("expected rope")
        };
        let second = first.clone().concat(leaf);
        let Storage::Rope(parent) = &second.0 else {
            panic!("expected rope")
        };
        assert_eq!(second.len(), ROPE_MIN_LENGTH * 2 + 1);
        assert!(!second.is_empty());
        assert_eq!(second, second.clone());
        assert!(child.flat.get().is_none());
        assert!(parent.flat.get().is_none());
        assert_eq!(second.code_units().len(), second.len());
        assert!(parent.flat.get().is_some());
        assert!(child.flat.get().is_none());
        let address = second.code_units().as_ptr();
        assert_eq!(second.clone().code_units().as_ptr(), address);
    }
}
