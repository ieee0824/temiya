//! String exotic objects and a non-JIT regular-expression matcher.

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use regex_automata::{
    Input, PatternID,
    meta::Regex,
    util::{captures::Captures, syntax},
};

use super::{JsString, Object, RuntimeError, RuntimeResult};

mod ascii_hir;

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
    captures: RefCell<Captures>,
    last_index: RefCell<usize>,
}

// Only immutable compiled strategies are reused. Templates are never searched;
// cloning a meta Regex creates a separate search-cache pool for the new object.
// The dependency reports approximate heap usage, so this is an accounting
// budget, not a hard process-memory limit. Entry count separately bounds keys.
const MATCHER_CACHE_BYTES: usize = 8 * 1024 * 1024;
const MATCHER_CACHE_ENTRIES: usize = 256;

struct CachedMatcher {
    source: String,
    flags: RegExpFlags,
    matcher: Regex,
    bytes: usize,
}

#[derive(Default)]
struct MatcherCache {
    entries: VecDeque<CachedMatcher>,
    bytes: usize,
}

thread_local! {
    static MATCHERS: RefCell<MatcherCache> = RefCell::new(MatcherCache::default());
}

fn compile_matcher(source: &str, flags: RegExpFlags) -> RuntimeResult<Regex> {
    if let Some(matcher) = MATCHERS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index = cache
            .entries
            .iter()
            .position(|entry| entry.flags == flags && entry.source == source)?;
        let entry = cache.entries.remove(index).expect("existing cache entry");
        let matcher = entry.matcher.clone();
        cache.entries.push_front(entry);
        Some(matcher)
    }) {
        return Ok(matcher);
    }
    let mut builder = Regex::builder();
    builder
        // Preserve the limits and UTF-8 policies of regex::RegexBuilder.
        .configure(
            Regex::config()
                .nfa_size_limit(Some(10 * (1 << 20)))
                .hybrid_cache_capacity(2 * (1 << 20))
                .utf8_empty(true),
        )
        .syntax(
            syntax::Config::new()
                .utf8(true)
                .case_insensitive(flags.ignore_case)
                .multi_line(flags.multiline)
                .dot_matches_new_line(flags.dot_all)
                .unicode(flags.unicode),
        );
    let direct = (!flags.ignore_case)
        .then(|| ascii_hir::parse(source))
        .flatten();
    let matcher = match direct {
        Some(hir) => builder.build_from_hir(&hir),
        None => builder.build(&translate_named_groups(source)),
    }
    .map_err(|error| RuntimeError::syntax_error(error.to_string()))?;
    let bytes = matcher
        .memory_usage()
        .saturating_add(matcher.create_cache().memory_usage())
        .saturating_add(source.len())
        .saturating_add(std::mem::size_of::<CachedMatcher>());
    if bytes <= MATCHER_CACHE_BYTES {
        MATCHERS.with(|cache| {
            let mut cache = cache.borrow_mut();
            while cache.entries.len() >= MATCHER_CACHE_ENTRIES
                || cache.bytes.saturating_add(bytes) > MATCHER_CACHE_BYTES
            {
                let entry = cache.entries.pop_back().expect("nonempty cache");
                cache.bytes -= entry.bytes;
            }
            cache.entries.push_front(CachedMatcher {
                source: source.to_owned(),
                flags,
                matcher: matcher.clone(),
                bytes,
            });
            cache.bytes += bytes;
        });
    }
    Ok(matcher)
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
        let matcher = compile_matcher(&source, flags)?;
        let captures = RefCell::new(matcher.create_captures());
        let object = Object::new();
        object.0.state.borrow_mut().regexp = Some(RegExpSlot(Rc::new(RegExpData {
            source: pattern,
            flags,
            matcher,
            captures,
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
        let mut captures = slot.0.captures.borrow_mut();
        slot.0.matcher.search_captures(
            &Input::new(&text).span(start_byte..text.len()),
            &mut captures,
        );
        if !captures.is_match()
            || (slot.0.flags.sticky
                && !captures
                    .get_group(0)
                    .is_some_and(|value| value.start == start_byte))
        {
            if stateful {
                *slot.0.last_index.borrow_mut() = 0;
            }
            return Ok(None);
        }
        let whole = captures.get_group(0).expect("capture zero");
        let index = byte_to_utf16(&text, whole.start);
        let values = (0..captures.group_len())
            .map(|index| {
                captures
                    .get_group(index)
                    .map(|value| JsString::from(&text[value.range()]))
            })
            .collect();
        let named_captures = slot
            .0
            .matcher
            .group_info()
            .pattern_names(PatternID::ZERO)
            .flatten()
            .map(|name| {
                (
                    name.to_owned(),
                    captures
                        .get_group_by_name(name)
                        .map(|value| JsString::from(&text[value.range()])),
                )
            })
            .collect();
        if stateful {
            let mut end = byte_to_utf16(&text, whole.end);
            if whole.start == whole.end {
                end += if slot.0.flags.unicode {
                    text[whole.end..].chars().next().map_or(1, char::len_utf16)
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

#[cfg(test)]
mod cache_tests {
    use super::*;

    // This allocates hundreds of thousands of automaton states to exercise the
    // byte budget. The smaller-pattern eviction/state test also runs in Miri.
    #[test]
    #[cfg(not(miri))]
    fn compiled_size_budget_evicts_before_the_entry_limit() {
        MATCHERS.with(|cache| *cache.borrow_mut() = MatcherCache::default());
        for i in 0..32 {
            RegExp::new(JsString::from(format!("a{{16384}}suffix{i}")), "").unwrap();
            MATCHERS.with(|cache| {
                let cache = cache.borrow();
                assert!(cache.bytes <= MATCHER_CACHE_BYTES);
                assert!(cache.entries.len() <= MATCHER_CACHE_ENTRIES);
            });
        }
        MATCHERS.with(|cache| {
            let cache = cache.borrow();
            assert!(!cache.entries.is_empty());
            assert!(cache.entries.len() < 32, "byte budget must cause eviction");
            println!(
                "byte-budget stress: {} entries, {} accounted bytes",
                cache.entries.len(),
                cache.bytes
            );
        });
    }

    #[test]
    fn unique_patterns_evict_without_changing_live_objects() {
        MATCHERS.with(|cache| *cache.borrow_mut() = MatcherCache::default());
        let original = RegExp::new(JsString::from("(?<keep>a+)"), "g").unwrap();
        let input = JsString::from("aa aa");
        original.exec(&input).unwrap().unwrap();
        for i in 0..MATCHER_CACHE_ENTRIES * 2 {
            let pattern = format!("prefix{i}([a-z]+)");
            let regexp = RegExp::new(JsString::from(pattern), "").unwrap();
            assert!(
                regexp
                    .exec(&JsString::from(format!("prefix{i}word")))
                    .unwrap()
                    .is_some()
            );
            MATCHERS.with(|cache| {
                let cache = cache.borrow();
                assert!(cache.entries.len() <= MATCHER_CACHE_ENTRIES);
                assert!(cache.bytes <= MATCHER_CACHE_BYTES);
            });
        }
        MATCHERS.with(|cache| {
            assert!(
                !cache
                    .borrow()
                    .entries
                    .iter()
                    .any(|entry| entry.source == "(?<keep>a+)")
            )
        });
        let matched = original.exec(&input).unwrap().unwrap();
        assert_eq!(matched.index, 3);
        assert_eq!(matched.named_captures[0].1, Some(JsString::from("aa")));
        let rebuilt = RegExp::new(JsString::from("(?<keep>a+)"), "g").unwrap();
        assert_eq!(rebuilt.last_index(), 0);
        assert_eq!(rebuilt.exec(&input).unwrap().unwrap().index, 0);
    }
}
