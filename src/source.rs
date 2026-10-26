//! Checked conversion between UTF-8 byte positions and UTF-16 source positions.

use crate::lexer::Span;

/// A source position using zero-based UTF-16 code units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourcePosition {
    /// UTF-16 code units from the start of the source.
    pub offset: usize,
    /// Zero-based line number.
    pub line: usize,
    /// UTF-16 code units since the beginning of this line.
    pub column: usize,
}

/// The endpoints of a half-open source range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceRange {
    /// Inclusive starting position.
    pub start: SourcePosition,
    /// Exclusive ending position.
    pub end: SourcePosition,
}

/// Why a source position cannot be converted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositionError {
    /// An offset exceeds the source length in its respective encoding.
    OutOfBounds,
    /// An offset splits a UTF-8 character or UTF-16 surrogate pair.
    InsideCharacter,
    /// The requested line or column does not exist at a character boundary.
    InvalidLineColumn,
    /// A range ends before it starts.
    ReversedRange,
}

impl std::fmt::Display for PositionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OutOfBounds => "source offset is out of bounds",
            Self::InsideCharacter => "source offset splits a character",
            Self::InvalidLineColumn => "invalid source line or column",
            Self::ReversedRange => "source range ends before it starts",
        })
    }
}

impl std::error::Error for PositionError {}

#[derive(Debug)]
struct Boundary {
    byte: usize,
    position: SourcePosition,
}

/// Borrowed UTF-8 source with an index of character boundaries.
///
/// Construction takes linear time and space; conversions use binary search.
/// LF, CR, CRLF, U+2028, and U+2029 separate lines. A CRLF completes one line
/// break after LF; the boundary between CR and LF is on the preceding line,
/// one UTF-16 column past CR's starting position. EOF is a valid position.
/// Boundaries inside non-BMP characters' surrogate pairs are rejected.
#[derive(Debug)]
pub struct SourceText<'a> {
    text: &'a str,
    boundaries: Vec<Boundary>,
}

impl<'a> SourceText<'a> {
    /// Indexes source text without copying its contents.
    #[must_use]
    pub fn new(text: &'a str) -> Self {
        let mut position = SourcePosition {
            offset: 0,
            line: 0,
            column: 0,
        };
        let mut boundaries = vec![Boundary { byte: 0, position }];
        let mut chars = text.char_indices().peekable();
        while let Some((byte, c)) = chars.next() {
            position.offset += c.len_utf16();
            if matches!(c, '\n' | '\u{2028}' | '\u{2029}')
                || (c == '\r' && !chars.peek().is_some_and(|(_, next)| *next == '\n'))
            {
                position.line += 1;
                position.column = 0;
            } else {
                position.column += c.len_utf16();
            }
            boundaries.push(Boundary {
                byte: byte + c.len_utf8(),
                position,
            });
        }
        Self { text, boundaries }
    }

    /// Returns the original source text.
    #[must_use]
    pub fn text(&self) -> &'a str {
        self.text
    }

    /// Converts a UTF-8 byte offset into a UTF-16 position.
    pub fn position(&self, byte: usize) -> Result<SourcePosition, PositionError> {
        if byte > self.text.len() {
            return Err(PositionError::OutOfBounds);
        }
        self.boundaries
            .binary_search_by_key(&byte, |b| b.byte)
            .map(|i| self.boundaries[i].position)
            .map_err(|_| PositionError::InsideCharacter)
    }

    /// Converts an absolute UTF-16 offset into a UTF-8 byte offset.
    pub fn byte_offset(&self, offset: usize) -> Result<usize, PositionError> {
        if offset > self.boundaries.last().unwrap().position.offset {
            return Err(PositionError::OutOfBounds);
        }
        self.boundaries
            .binary_search_by_key(&offset, |b| b.position.offset)
            .map(|i| self.boundaries[i].byte)
            .map_err(|_| PositionError::InsideCharacter)
    }

    /// Converts a zero-based line and UTF-16 column into a byte offset.
    ///
    /// Columns must identify a character boundary on the requested line.
    /// Newline bytes are not clamped to a line's visible content.
    pub fn byte_offset_at(&self, line: usize, column: usize) -> Result<usize, PositionError> {
        self.boundaries
            .binary_search_by_key(&(line, column), |b| (b.position.line, b.position.column))
            .map(|i| self.boundaries[i].byte)
            .map_err(|_| PositionError::InvalidLineColumn)
    }

    /// Converts a half-open byte span, rejecting reversed or invalid endpoints.
    pub fn range(&self, span: Span) -> Result<SourceRange, PositionError> {
        if span.start > span.end {
            return Err(PositionError::ReversedRange);
        }
        Ok(SourceRange {
            start: self.position(span.start)?,
            end: self.position(span.end)?,
        })
    }
}
