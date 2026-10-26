//! String exotic objects and a non-JIT regular-expression matcher.

use std::{cell::RefCell, rc::Rc};

use regex::{Regex, RegexBuilder};

use super::{JsString, Object, RuntimeError, RuntimeResult};

/// Parsed ECMAScript regular-expression flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RegExpFlags {
    /// Global matching (`g`).
    pub global: bool,
    /// Case-insensitive matching (`i`).
    pub ignore_case: bool,
    /// Multiline anchors (`m`).
    pub multiline: bool,
    /// Dot matches line terminators (`s`).
    pub dot_all: bool,
    /// Unicode-aware matching (`u`).
    pub unicode: bool,
    /// Sticky matching (`y`).
    pub sticky: bool,
}

impl RegExpFlags {
    /// Parses flags and rejects duplicates or unsupported spellings.
    pub fn parse(flags: &str) -> RuntimeResult<Self> {
        let mut result = Self::default();
        for flag in flags.chars() {
            let slot = match flag {
                'g' => &mut result.global,
                'i' => &mut result.ignore_case,
                'm' => &mut result.multiline,
                's' => &mut result.dot_all,
                'u' => &mut result.unicode,
                'y' => &mut result.sticky,
                _ => {
                    return Err(RuntimeError::syntax_error(format!(
                        "invalid RegExp flag {flag}"
                    )));
                }
            };
            if *slot {
                return Err(RuntimeError::syntax_error(format!(
                    "duplicate RegExp flag {flag}"
                )));
            }
            *slot = true;
        }
        Ok(result)
    }
}

#[derive(Clone)]
pub(super) struct RegExpSlot(Rc<RegExpData>);

struct RegExpData {
    source: JsString,
    flags: RegExpFlags,
    matcher: Regex,
    last_index: RefCell<usize>,
}

/// One successful regular-expression match and its captures.
#[derive(Clone, Debug, PartialEq)]
pub struct RegExpMatch {
    /// UTF-16 start index.
    pub index: usize,
    /// Full match followed by numbered captures; unmatched captures are absent.
    pub captures: Vec<Option<JsString>>,
    /// Named captures in declaration order.
    pub named_captures: Vec<(String, Option<JsString>)>,
}

/// A compiled ECMAScript-like regular expression with observable `lastIndex`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegExp(Object);

impl RegExp {
    /// Compiles a pattern and its flags.
    pub fn new(pattern: JsString, flags: &str) -> RuntimeResult<Self> {
        let flags = RegExpFlags::parse(flags)?;
        let source = scalar_string(&pattern)?;
        let translated = translate_named_groups(&source);
        let matcher = RegexBuilder::new(&translated)
            .case_insensitive(flags.ignore_case)
            .multi_line(flags.multiline)
            .dot_matches_new_line(flags.dot_all)
            .unicode(flags.unicode)
            .build()
            .map_err(|error| RuntimeError::syntax_error(error.to_string()))?;
        let object = Object::new();
        object.0.state.borrow_mut().regexp = Some(RegExpSlot(Rc::new(RegExpData {
            source: pattern,
            flags,
            matcher,
            last_index: RefCell::new(0),
        })));
        Ok(Self(object))
    }

    /// Parses an original literal spelling such as `/a+/gu`.
    pub fn from_literal(spelling: &str) -> RuntimeResult<Self> {
        let end = spelling
            .rfind('/')
            .filter(|index| *index > 0)
            .ok_or_else(|| RuntimeError::syntax_error("invalid RegExp literal"))?;
        Self::new(
            JsString::from(spelling[1..end].to_owned()),
            &spelling[end + 1..],
        )
    }

    /// Wraps an object with RegExp internal slots.
    #[must_use]
    pub fn from_object(object: Object) -> Option<Self> {
        let present = object.0.state.borrow().regexp.is_some();
        present.then_some(Self(object))
    }

    /// Returns the underlying JavaScript object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }

    /// Returns the source pattern.
    #[must_use]
    pub fn source(&self) -> JsString {
        self.slot().0.source.clone()
    }

    /// Returns the current UTF-16 `lastIndex`.
    #[must_use]
    pub fn last_index(&self) -> usize {
        *self.slot().0.last_index.borrow()
    }

    /// Sets the UTF-16 `lastIndex`.
    pub fn set_last_index(&self, index: usize) {
        *self.slot().0.last_index.borrow_mut() = index;
    }

    /// Executes the matcher and updates `lastIndex` for global or sticky expressions.
    pub fn exec(&self, input: &JsString) -> RuntimeResult<Option<RegExpMatch>> {
        let slot = self.slot();
        let text = scalar_string(input)?;
        let stateful = slot.0.flags.global || slot.0.flags.sticky;
        let start_utf16 = if stateful {
            *slot.0.last_index.borrow()
        } else {
            0
        };
        let Some(start_byte) = utf16_to_byte(&text, start_utf16) else {
            if stateful {
                *slot.0.last_index.borrow_mut() = 0;
            }
            return Ok(None);
        };
        let captures = slot.0.matcher.captures_at(&text, start_byte);
        let Some(captures) = captures.filter(|captures| {
            !slot.0.flags.sticky
                || captures
                    .get(0)
                    .is_some_and(|value| value.start() == start_byte)
        }) else {
            if stateful {
                *slot.0.last_index.borrow_mut() = 0;
            }
            return Ok(None);
        };
        let whole = captures.get(0).expect("capture zero");
        let index = byte_to_utf16(&text, whole.start());
        let values = (0..captures.len())
            .map(|index| {
                captures
                    .get(index)
                    .map(|value| JsString::from(value.as_str()))
            })
            .collect();
        let named_captures = slot
            .0
            .matcher
            .capture_names()
            .flatten()
            .map(|name| {
                (
                    name.to_owned(),
                    captures
                        .name(name)
                        .map(|value| JsString::from(value.as_str())),
                )
            })
            .collect();
        if stateful {
            let mut end = byte_to_utf16(&text, whole.end());
            if whole.start() == whole.end() {
                end += if slot.0.flags.unicode {
                    text[whole.end()..]
                        .chars()
                        .next()
                        .map_or(1, char::len_utf16)
                } else {
                    1
                };
            }
            *slot.0.last_index.borrow_mut() = end;
        }
        Ok(Some(RegExpMatch {
            index,
            captures: values,
            named_captures,
        }))
    }

    /// Replaces matches, invoking a callback with the matched text and capture values.
    pub fn replace(
        &self,
        input: &JsString,
        callback: impl Fn(&RegExpMatch) -> RuntimeResult<JsString>,
    ) -> RuntimeResult<JsString> {
        let text = scalar_string(input)?;
        let global = self.slot().0.flags.global;
        if global {
            self.set_last_index(0);
        }
        let mut output = String::new();
        let mut copied_utf16 = 0;
        while let Some(found) = self.exec(input)? {
            let matched = found.captures[0].as_ref().expect("whole match");
            let end = found.index + matched.code_units().len();
            output.push_str(&utf16_range(&text, copied_utf16, found.index)?);
            output.push_str(&scalar_string(&callback(&found)?)?);
            copied_utf16 = end;
            if !global {
                break;
            }
        }
        output.push_str(&utf16_range(&text, copied_utf16, input.code_units().len())?);
        Ok(JsString::from(output))
    }

    fn slot(&self) -> RegExpSlot {
        self.0.0.state.borrow().regexp.clone().expect("RegExp slot")
    }
}

/// A boxed String exotic object with UTF-16 indexed own properties.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StringObject(Object);

impl StringObject {
    /// Boxes a string value.
    #[must_use]
    pub fn new(value: JsString) -> Self {
        let object = Object::new();
        object.0.state.borrow_mut().string_data = Some(value);
        Self(object)
    }

    /// Returns the underlying object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.0.clone()
    }

    /// Returns the boxed string data.
    #[must_use]
    pub fn value(&self) -> JsString {
        self.0
            .0
            .state
            .borrow()
            .string_data
            .clone()
            .expect("String data")
    }
}

fn translate_named_groups(pattern: &str) -> String {
    pattern.replace("(?<", "(?P<")
}

fn scalar_string(value: &JsString) -> RuntimeResult<String> {
    String::from_utf16(value.code_units())
        .map_err(|_| RuntimeError::type_error("unpaired surrogate is not supported by RegExp"))
}

fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

fn utf16_to_byte(text: &str, index: usize) -> Option<usize> {
    if index == text.encode_utf16().count() {
        return Some(text.len());
    }
    text.char_indices()
        .find_map(|(byte, _)| (byte_to_utf16(text, byte) == index).then_some(byte))
}

fn utf16_range(text: &str, start: usize, end: usize) -> RuntimeResult<String> {
    let start = utf16_to_byte(text, start)
        .ok_or_else(|| RuntimeError::range_error("invalid UTF-16 boundary"))?;
    let end = utf16_to_byte(text, end)
        .ok_or_else(|| RuntimeError::range_error("invalid UTF-16 boundary"))?;
    Ok(text[start..end].to_owned())
}
