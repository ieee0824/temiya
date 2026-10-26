# Source positions

`temiya::source::SourceText` borrows UTF-8 text and indexes character boundaries
in linear time and space. `position(byte)` converts a byte offset to an absolute
UTF-16 offset and zero-based line and UTF-16 column. `byte_offset(offset)` and
`byte_offset_at(line, column)` provide the inverse conversions using binary
search. `range(lexer::Span)` converts both endpoints of a half-open byte range.

EOF and empty ranges are valid. Offsets outside the source, UTF-8 character
interiors, UTF-16 surrogate interiors, invalid line/column pairs, and reversed
ranges return explicit `PositionError` values rather than clamping or panicking.

LF, CR, CRLF, U+2028, and U+2029 separate lines. CRLF advances the line once,
after LF. The character boundary between CR and LF belongs to the preceding
line, one column after CR begins. This convention gives every character boundary
a unique line/column pair and permits exact round trips, including newline
interiors and EOF. Columns are UTF-16 code units, not rendered screen columns;
tabs count as one code unit. This API does not produce source maps or implement
CDP messages, and does not accept unpaired surrogates in its UTF-8 source.
