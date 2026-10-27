//! Allocation-light translation of a conservative ASCII grammar to matcher HIR.
//! An unrecognized form returns None, leaving validation to the normal parser.
use regex_syntax::hir::{Capture, Class, ClassBytes, ClassBytesRange, Hir, Repetition};

#[cfg(test)]
pub(super) fn parse(source: &str) -> Option<Hir> {
    parse_units(&source.encode_utf16().collect::<Vec<_>>())
}

pub(super) fn parse_units(source: &[u16]) -> Option<Hir> {
    if source.iter().any(|unit| *unit >= 128) {
        return None;
    }
    let mut parser = Parser {
        bytes: source,
        offset: 0,
        next_capture: 1,
    };
    let hir = parser.expression(0)?;
    (parser.offset == parser.bytes.len()).then_some(hir)
}

struct Parser<'a> {
    bytes: &'a [u16],
    offset: usize,
    next_capture: u32,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).map(|unit| *unit as u8)
    }

    fn expression(&mut self, depth: usize) -> Option<Hir> {
        let first = self.branch(depth)?;
        if self.peek() != Some(b'|') {
            return Some(first);
        }
        let mut alternatives = vec![first];
        while self.peek() == Some(b'|') {
            self.offset += 1;
            alternatives.push(self.branch(depth)?);
        }
        Some(Hir::alternation(alternatives))
    }

    fn branch(&mut self, depth: usize) -> Option<Hir> {
        let mut nodes = Vec::new();
        while !matches!(self.peek(), None | Some(b'|' | b')')) {
            let atom = self.atom(depth)?;
            nodes.push(self.repetition(atom)?);
        }
        Some(Hir::concat(nodes))
    }

    fn atom(&mut self, depth: usize) -> Option<Hir> {
        match self.peek()? {
            b'(' => {
                if depth >= 32 {
                    return None;
                }
                self.offset += 1;
                let capture = if self.peek() == Some(b'?') {
                    if self.bytes.get(self.offset + 1) != Some(&u16::from(b':')) {
                        return None;
                    }
                    self.offset += 2;
                    None
                } else {
                    let index = self.next_capture;
                    self.next_capture = index.checked_add(1)?;
                    Some(index)
                };
                let sub = self.expression(depth + 1)?;
                if self.peek() != Some(b')') {
                    return None;
                }
                self.offset += 1;
                Some(match capture {
                    Some(index) => Hir::capture(Capture {
                        index,
                        name: None,
                        sub: Box::new(sub),
                    }),
                    None => sub,
                })
            }
            b'[' => self.class(),
            byte if literal(byte) => {
                let start = self.offset;
                let mut end = start + 1;
                while self.bytes.get(end).is_some_and(|b| literal(*b as u8)) {
                    end += 1;
                }
                // A following quantifier binds to the last byte, not the run.
                if end > start + 1
                    && self
                        .bytes
                        .get(end)
                        .is_some_and(|b| matches!(*b as u8, b'*' | b'+' | b'?' | b'{'))
                {
                    end -= 1;
                }
                self.offset = end;
                Some(Hir::literal(
                    self.bytes[start..end]
                        .iter()
                        .map(|unit| *unit as u8)
                        .collect::<Vec<_>>(),
                ))
            }
            _ => None,
        }
    }

    fn class(&mut self) -> Option<Hir> {
        self.offset += 1;
        let mut ranges = Vec::new();
        while self.peek()? != b']' {
            let start = self.class_byte()?;
            let end = if self.peek() == Some(b'-') {
                self.offset += 1;
                self.class_byte()?
            } else {
                start
            };
            if end < start {
                return None;
            }
            ranges.push(ClassBytesRange::new(start, end));
        }
        if ranges.is_empty() {
            return None;
        }
        self.offset += 1;
        Some(Hir::class(Class::Bytes(ClassBytes::new(ranges))))
    }

    fn class_byte(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b' ')) {
            return None;
        }
        self.offset += 1;
        Some(byte)
    }

    fn number(&mut self) -> Option<u32> {
        let start = self.offset;
        let mut n = 0_u32;
        while let Some(byte) = self.peek().filter(u8::is_ascii_digit) {
            n = n.checked_mul(10)?.checked_add(u32::from(byte - b'0'))?;
            self.offset += 1;
        }
        (self.offset > start).then_some(n)
    }

    fn repetition(&mut self, sub: Hir) -> Option<Hir> {
        let (min, max) = match self.peek() {
            Some(b'?') => {
                self.offset += 1;
                (0, Some(1))
            }
            Some(b'*') => {
                self.offset += 1;
                (0, None)
            }
            Some(b'+') => {
                self.offset += 1;
                (1, None)
            }
            Some(b'{') => {
                self.offset += 1;
                let min = self.number()?;
                let max = match self.peek()? {
                    b'}' => Some(min),
                    b',' => {
                        self.offset += 1;
                        if self.peek() == Some(b'}') {
                            None
                        } else {
                            Some(self.number()?)
                        }
                    }
                    _ => return None,
                };
                if self.peek() != Some(b'}') || max.is_some_and(|max| max < min) {
                    return None;
                }
                self.offset += 1;
                (min, max)
            }
            _ => return Some(sub),
        };
        let greedy = self.peek() != Some(b'?');
        if !greedy {
            self.offset += 1;
        }
        Some(Hir::repetition(Repetition {
            min,
            max,
            greedy,
            sub: Box::new(sub),
        }))
    }
}

fn literal(byte: u8) -> bool {
    !matches!(
        byte,
        b'(' | b')'
            | b'['
            | b']'
            | b'\\'
            | b'.'
            | b'^'
            | b'$'
            | b'|'
            | b'*'
            | b'+'
            | b'?'
            | b'{'
            | b'}'
    )
}

#[cfg(test)]
mod tests {
    use super::super::matcher::Matcher;
    use super::*;
    use regex_automata::{Input, PatternID};

    #[test]
    fn supported_grammar_matches_full_parser_captures_and_offsets() {
        let patterns = [
            "",
            "a",
            "abc",
            "ab+c",
            "ab{2}c",
            "[a-z]+",
            "[a-zA-Z0-9_]+",
            "(a)?b",
            "(?:ab)*",
            "((a)b)+",
            "(a|b)+",
            "a|",
            "a|ab",
            "ab|a",
            "(a|ab)(b?)",
            "a+?",
            "a*?",
            "a??",
            "|a",
            "()",
            "(a){0}",
            "(a){0,1}",
            "a{2,}",
            "a{1,3}?",
            "([a-z]+)-([0-9]+)(?:unique123)?",
            "([a-z]+)-([0-9]+)(?:x{127})?",
        ];
        for pattern in patterns {
            for unicode in [false, true] {
                let hir = parse(pattern).expect(pattern);
                let fast = Matcher::from_hir(&hir).unwrap();
                let mut scratch = fast.create_scratch();
                let reference = regex::RegexBuilder::new(pattern)
                    .unicode(unicode)
                    .build()
                    .unwrap();
                assert_eq!(
                    fast.group_info().group_len(PatternID::ZERO),
                    reference.captures_len(),
                    "{pattern}"
                );
                for input in [
                    "",
                    "a",
                    "b",
                    "ab",
                    "aaab",
                    "abc",
                    "abbbc",
                    "alpha-42",
                    "alpha-42unique123",
                    "abc-42xxx",
                    "😀ab",
                    "a\nb",
                    "_",
                    "1234",
                ] {
                    for start in [0, input.len()] {
                        let mut got = fast.create_captures();
                        fast.search(
                            &Input::new(input).span(start..input.len()),
                            &mut got,
                            &mut scratch,
                        );
                        let expected = reference.captures_at(input, start);
                        assert_eq!(
                            got.is_match(),
                            expected.is_some(),
                            "{pattern:?} {input:?} start={start} unicode={unicode}"
                        );
                        if let Some(expected) = expected {
                            let a: Vec<_> = (0..got.group_len())
                                .map(|i| got.get_group(i).map(|m| (m.start, m.end)))
                                .collect();
                            let b: Vec<_> = (0..expected.len())
                                .map(|i| expected.get(i).map(|m| (m.start(), m.end())))
                                .collect();
                            assert_eq!(
                                a, b,
                                "{pattern:?} {input:?} start={start} unicode={unicode}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn unsupported_and_ambiguous_forms_keep_full_parser_validation() {
        for pattern in [
            ".",
            r"\w+",
            "[^a]",
            "(?<name>a)",
            "(?P<name>a)",
            "(?i:a)",
            "a**",
            "a{999999999999}",
            "[a-]",
            "[z-a]",
            "[a&&b]",
            "😀",
            "a[",
            "(a",
            "a)",
        ] {
            assert!(parse(pattern).is_none(), "{pattern}");
        }
        assert!(parse(&format!("{}a{}", "(".repeat(33), ")".repeat(33))).is_none());
    }
}
