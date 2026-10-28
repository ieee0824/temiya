//! String exotic objects and a non-JIT regular-expression matcher.

use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::RandomState},
    hash::{BuildHasher, Hash, Hasher},
    rc::Rc,
};

use regex_automata::{
    Input, PatternID,
    meta::Regex,
    util::{captures::Captures, syntax},
};

use super::{JsString, Object, RuntimeError, RuntimeResult, Value};

mod ascii_hir;
mod matcher;
use matcher::{Matcher, Scratch};

/// Parsed ECMAScript regular-expression flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
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
    matcher: Matcher,
    captures: RefCell<Captures>,
    scratch: RefCell<Scratch>,
    last_index: RefCell<usize>,
}

// Only immutable compiled strategies are reused. Templates are never searched;
// cloning a meta Regex creates a separate search-cache pool for the new object.
// The dependency reports approximate heap usage, so this is an accounting
// budget, not a hard process-memory limit. Entry count separately bounds keys.
const MATCHER_CACHE_BYTES: usize = 8 * 1024 * 1024;
const MATCHER_CACHE_ENTRIES: usize = 256;

struct CachedMatcher {
    source: Vec<u16>,
    flags: RegExpFlags,
    matcher: Matcher,
    bytes: usize,
    hash: u64,
    previous: Option<usize>,
    next: Option<usize>,
    collision_next: Option<usize>,
}

#[derive(Default)]
struct MatcherCache {
    entries: Vec<Option<CachedMatcher>>,
    index: HashMap<u64, usize>,
    free: Vec<usize>,
    head: Option<usize>,
    tail: Option<usize>,
    len: usize,
    bytes: usize,
    hash_builder: RandomState,
}

impl MatcherCache {
    fn key_hash(&self, source: &[u16], flags: RegExpFlags) -> u64 {
        let mut hasher = self.hash_builder.build_hasher();
        source.hash(&mut hasher);
        flags.hash(&mut hasher);
        hasher.finish()
    }

    fn get_hashed(&mut self, source: &[u16], flags: RegExpFlags, hash: u64) -> Option<Matcher> {
        let mut index = *self.index.get(&hash)?;
        loop {
            let entry = self.entries[index].as_ref().expect("indexed cache entry");
            if entry.flags == flags && entry.source == source {
                let matcher = entry.matcher.clone();
                self.unlink(index);
                self.push_front(index);
                return Some(matcher);
            }
            index = entry.collision_next?;
        }
    }

    fn unlink(&mut self, index: usize) {
        let entry = self.entries[index].as_ref().expect("linked cache entry");
        let (previous, next) = (entry.previous, entry.next);
        if let Some(previous) = previous {
            self.entries[previous]
                .as_mut()
                .expect("previous entry")
                .next = next;
        } else {
            self.head = next;
        }
        if let Some(next) = next {
            self.entries[next].as_mut().expect("next entry").previous = previous;
        } else {
            self.tail = previous;
        }
    }

    fn push_front(&mut self, index: usize) {
        let entry = self.entries[index].as_mut().expect("new head entry");
        entry.previous = None;
        entry.next = self.head;
        if let Some(head) = self.head {
            self.entries[head]
                .as_mut()
                .expect("old head entry")
                .previous = Some(index);
        } else {
            self.tail = Some(index);
        }
        self.head = Some(index);
    }

    fn evict_tail(&mut self) {
        let index = self.tail.expect("nonempty cache");
        self.unlink(index);
        let entry = self.entries[index].take().expect("evicted cache entry");
        let head = self.index[&entry.hash];
        if head == index {
            if let Some(next) = entry.collision_next {
                self.index.insert(entry.hash, next);
            } else {
                self.index.remove(&entry.hash);
            }
        } else {
            let mut previous = head;
            loop {
                let parent = self.entries[previous]
                    .as_mut()
                    .expect("collision chain entry");
                if parent.collision_next == Some(index) {
                    parent.collision_next = entry.collision_next;
                    break;
                }
                previous = parent.collision_next.expect("indexed collision target");
            }
        }
        self.bytes -= entry.bytes;
        self.len -= 1;
        self.free.push(index);
    }

    fn insert_hashed(
        &mut self,
        source: &[u16],
        flags: RegExpFlags,
        matcher: Matcher,
        bytes: usize,
        hash: u64,
    ) {
        while self.len >= MATCHER_CACHE_ENTRIES
            || self.bytes.saturating_add(bytes) > MATCHER_CACHE_BYTES
        {
            self.evict_tail();
        }
        let index = self.free.pop().unwrap_or_else(|| {
            self.entries.push(None);
            self.entries.len() - 1
        });
        let collision_next = self.index.insert(hash, index);
        self.entries[index] = Some(CachedMatcher {
            source: source.to_owned(),
            flags,
            matcher,
            bytes,
            hash,
            previous: None,
            next: None,
            collision_next,
        });
        self.push_front(index);
        self.len += 1;
        self.bytes += bytes;
    }
}

thread_local! {
    static MATCHERS: RefCell<MatcherCache> = RefCell::new(MatcherCache::default());
}

fn compile_matcher(pattern: &JsString, flags: RegExpFlags) -> RuntimeResult<Matcher> {
    let units = pattern.code_units();
    // ASCII proves scalar validity; other patterns retain validation before lookup.
    let scalar = if units.iter().all(|unit| *unit < 128) {
        None
    } else {
        Some(scalar_string(pattern)?)
    };
    let (hash, cached) = MATCHERS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let hash = cache.key_hash(units, flags);
        (hash, cache.get_hashed(units, flags, hash))
    });
    if let Some(matcher) = cached {
        return Ok(matcher);
    }
    if !flags.ignore_case {
        if let Some(hir) = ascii_hir::parse_units(units) {
            return finish_matcher(units, flags, hash, Matcher::from_hir(&hir, units.len())?);
        }
    }
    let source = match scalar {
        Some(source) => source,
        None => scalar_string(pattern)?,
    };
    let matcher = Regex::builder()
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
        )
        .build(&translate_named_groups(&source))
        .map_err(|error| RuntimeError::syntax_error(error.to_string()))?;
    finish_matcher(units, flags, hash, Matcher::Meta(matcher))
}

fn finish_matcher(
    source: &[u16],
    flags: RegExpFlags,
    hash: u64,
    matcher: Matcher,
) -> RuntimeResult<Matcher> {
    let bytes = matcher
        .template_memory_usage()
        .saturating_add(source.len().saturating_mul(std::mem::size_of::<u16>()))
        .saturating_add(std::mem::size_of::<CachedMatcher>());
    if bytes <= MATCHER_CACHE_BYTES {
        MATCHERS.with(|cache| {
            cache
                .borrow_mut()
                .insert_hashed(source, flags, matcher.clone(), bytes, hash)
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
        Self::allocate(None, pattern, flags)
    }

    pub(crate) fn new_for(context: &Object, pattern: JsString, flags: &str) -> RuntimeResult<Self> {
        Self::allocate(Some(context), pattern, flags)
    }

    fn allocate(context: Option<&Object>, pattern: JsString, flags: &str) -> RuntimeResult<Self> {
        let flags = RegExpFlags::parse(flags)?;
        let matcher = compile_matcher(&pattern, flags)?;
        let captures = RefCell::new(matcher.create_captures());
        let scratch = RefCell::new(matcher.create_scratch());
        let object = context.map_or_else(Object::new, Object::new_for);
        object.0.state.borrow_mut().regexp = Some(RegExpSlot(Rc::new(RegExpData {
            source: pattern,
            flags,
            matcher,
            captures,
            scratch,
            last_index: RefCell::new(0),
        })));
        Ok(Self(object))
    }

    /// Parses an original literal spelling such as `/a+/gu`.
    pub fn from_literal(spelling: &str) -> RuntimeResult<Self> {
        Self::literal_in(None, spelling)
    }

    pub(crate) fn from_literal_for(context: &Object, spelling: &str) -> RuntimeResult<Self> {
        Self::literal_in(Some(context), spelling)
    }

    fn literal_in(context: Option<&Object>, spelling: &str) -> RuntimeResult<Self> {
        let end = spelling
            .rfind('/')
            .filter(|index| *index > 0)
            .ok_or_else(|| RuntimeError::syntax_error("invalid RegExp literal"))?;
        Self::allocate(
            context,
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
        self.exec_with(input, |index, text, captures, matcher| {
            let values = (0..captures.group_len())
                .map(|index| {
                    captures
                        .get_group(index)
                        .map(|value| JsString::from(&text[value.range()]))
                })
                .collect();
            let named_captures = matcher
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

            RegExpMatch {
                index,
                captures: values,
                named_captures,
            }
        })
    }

    pub(super) fn exec_values(&self, input: &JsString) -> RuntimeResult<Option<Vec<Value>>> {
        self.exec_with(input, |_, text, captures, _| {
            (0..captures.group_len())
                .map(|index| {
                    captures.get_group(index).map_or(Value::Undefined, |span| {
                        Value::String(JsString::from(&text[span.range()]))
                    })
                })
                .collect()
        })
    }

    // Internal ownership conversion only: neither callback executes JavaScript.
    fn exec_with<T>(
        &self,
        input: &JsString,
        finish: impl FnOnce(usize, &str, &Captures, &Matcher) -> T,
    ) -> RuntimeResult<Option<T>> {
        let slot = self.slot();
        let units = input.code_units();
        let mut ascii = [0_u8; 64];
        let short_ascii = units.len() <= ascii.len()
            && units.iter().zip(ascii.iter_mut()).all(|(unit, byte)| {
                if *unit >= 128 {
                    return false;
                }
                *byte = *unit as u8;
                true
            });
        let owned;
        let text = if short_ascii {
            std::str::from_utf8(&ascii[..units.len()]).expect("validated ASCII input")
        } else {
            owned = scalar_string(input)?;
            owned.as_str()
        };
        let stateful = slot.0.flags.global || slot.0.flags.sticky;
        let start_utf16 = if stateful {
            *slot.0.last_index.borrow()
        } else {
            0
        };
        let Some(start_byte) = utf16_to_byte(text, start_utf16) else {
            if stateful {
                *slot.0.last_index.borrow_mut() = 0;
            }
            return Ok(None);
        };
        let mut captures = slot.0.captures.borrow_mut();
        slot.0.matcher.search(
            &Input::new(text).span(start_byte..text.len()),
            &mut captures,
            &mut slot.0.scratch.borrow_mut(),
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
        let index = byte_to_utf16(text, whole.start);
        if stateful {
            let mut end = byte_to_utf16(text, whole.end);
            if whole.start == whole.end {
                end += if slot.0.flags.unicode {
                    text[whole.end..].chars().next().map_or(1, char::len_utf16)
                } else {
                    1
                };
            }
            *slot.0.last_index.borrow_mut() = end;
        }
        Ok(Some(finish(index, text, &captures, &slot.0.matcher)))
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

    pub(crate) fn new_for(context: &Object, value: JsString) -> Self {
        let object = Object::new_for(context);
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
    #[test]
    fn managed_regexp_instances_keep_cursors_separate_and_release_leaf_state() {
        let runtime = crate::runtime::Runtime::new();
        let context = Object::new_in(&runtime);
        let first = RegExp::new_for(&context, "(a+)".into(), "g").unwrap();
        let second = RegExp::new_for(&context, "(a+)".into(), "g").unwrap();
        let weak_data = Rc::downgrade(&first.slot().0);
        assert!(first.exec(&"aa".into()).unwrap().is_some());
        assert_eq!(first.last_index(), 2);
        assert_eq!(second.last_index(), 0);
        let before = runtime.diagnostics().live;
        assert!(RegExp::new_for(&context, "(".into(), "").is_err());
        assert_eq!(runtime.diagnostics().live, before);
        drop(context);
        runtime.collect_migration_graph().unwrap();
        assert_eq!(first.last_index(), 2);
        assert!(second.exec(&"aa".into()).unwrap().is_some());
        assert!(first.exec(&"aa".into()).unwrap().is_none());
        assert_eq!(first.last_index(), 0);
        drop(first);
        runtime.collect_migration_graph().unwrap();
        assert!(weak_data.upgrade().is_none());
        drop(second);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }
    fn units(source: &str) -> Vec<u16> {
        source.encode_utf16().collect()
    }

    #[test]
    fn hash_collisions_check_full_keys_and_evict_both_chain_positions() {
        let flags = RegExpFlags::default();
        let matcher =
            |source| Matcher::from_hir(&ascii_hir::parse(source).unwrap(), source.len()).unwrap();
        for evict_hash_head in [false, true] {
            let mut cache = MatcherCache::default();
            cache.insert_hashed(&units("a"), flags, matcher("a"), 100, 7);
            cache.insert_hashed(&units("b"), flags, matcher("b"), 100, 7);
            assert!(cache.get_hashed(&units("absent"), flags, 7).is_none());
            assert!(
                cache
                    .get_hashed(
                        &units("a"),
                        RegExpFlags {
                            global: true,
                            ..flags
                        },
                        7
                    )
                    .is_none()
            );
            if evict_hash_head {
                assert!(cache.get_hashed(&units("a"), flags, 7).is_some());
            }
            cache.evict_tail();
            let (remaining, removed, start) = if evict_hash_head {
                ("a", "b", 0)
            } else {
                ("b", "a", 1)
            };
            assert!(cache.get_hashed(&units(removed), flags, 7).is_none());
            let selected = cache.get_hashed(&units(remaining), flags, 7).unwrap();
            let mut captures = selected.create_captures();
            selected.search(
                &Input::new("ab"),
                &mut captures,
                &mut selected.create_scratch(),
            );
            assert_eq!(captures.get_group(0).unwrap().start, start);
            cache.evict_tail();
            assert_eq!(cache.len, 0);
            assert_eq!(cache.bytes, 0);
            assert!(cache.index.is_empty());
            assert_eq!(cache.head, None);
            assert_eq!(cache.tail, None);
            cache.insert_hashed(&units("c"), flags, matcher("c"), 100, 7);
            assert!(cache.get_hashed(&units("c"), flags, 7).is_some());
            assert_eq!(cache.entries.len(), 2, "reuse vacated slots");
        }
    }

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
                assert!(cache.len <= MATCHER_CACHE_ENTRIES);
            });
        }
        MATCHERS.with(|cache| {
            let cache = cache.borrow();
            assert!(cache.len != 0);
            assert!(cache.len < 32, "byte budget must cause eviction");
            println!(
                "byte-budget stress: {} entries, {} accounted bytes",
                cache.len, cache.bytes
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
                assert!(cache.len <= MATCHER_CACHE_ENTRIES);
                assert!(cache.bytes <= MATCHER_CACHE_BYTES);
            });
        }
        MATCHERS.with(|cache| {
            assert!(
                !cache
                    .borrow()
                    .entries
                    .iter()
                    .flatten()
                    .any(|entry| entry.source == units("(?<keep>a+)"))
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
