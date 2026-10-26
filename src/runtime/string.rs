//! Immutable UTF-16 storage: short names are inline; longer clones share a buffer.
use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};

// Fit inline units and tags in the former Vec footprint on 32/64 bit targets.
const INLINE_CAPACITY: usize = (std::mem::size_of::<Vec<u16>>() - 4) / 2;

#[derive(Clone)]
enum Storage {
    Inline {
        len: u8,
        units: [u16; INLINE_CAPACITY],
    },
    // Preserve the Vec's allocation instead of copying the whole result into
    // a new Arc<[u16]> allocation. All access to this buffer is immutable.
    Shared(Arc<Vec<u16>>),
}

/// A JavaScript String represented as an exact, immutable sequence of UTF-16 code units.
#[derive(Clone)]
pub struct JsString(Storage);

impl JsString {
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
    pub fn code_units(&self) -> &[u16] {
        match &self.0 {
            Storage::Inline { len, units } => &units[..usize::from(*len)],
            Storage::Shared(units) => units,
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
        self.code_units().is_empty()
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
    fn from(value: &str) -> Self {
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
    fn eq(&self, other: &Self) -> bool {
        if let (Storage::Shared(left), Storage::Shared(right)) = (&self.0, &other.0) {
            if Arc::ptr_eq(left, right) {
                return true;
            }
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
