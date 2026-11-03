use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::SourceFileId;

/// A position in a source file.
///
/// `line` and `column` are 1-based, the column counted in characters; `offset` is the 0-based
/// byte offset. Positions order by line, then column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Pos {
    /// The line, from 1.
    pub line: u32,
    /// The column, from 1, in characters.
    pub column: u32,
    /// The byte offset from the start of the file, from 0.
    pub offset: usize,
}

impl Pos {
    /// The start of a file: line 1, column 1, offset 0.
    pub const START: Self = Self { line: 1, column: 1, offset: 0 };

    /// A position from its 1-based line and column and its byte offset.
    #[must_use]
    pub const fn new(line: u32, column: u32, offset: usize) -> Self {
        Self { line, column, offset }
    }
}

/// Where something came from: a file and a start and end position in it.
///
/// Spans order by file, then start, then end, which is the order diagnostics are shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceSpan {
    /// The file, as an index into the program's file table.
    pub file: SourceFileId,
    /// The first position covered.
    pub start: Pos,
    /// The position just after the last one covered.
    pub end: Pos,
}

impl SourceSpan {
    /// The span between `start` and `end` in `file`; the two are swapped when `end` comes first.
    #[must_use]
    pub fn new(file: SourceFileId, start: Pos, end: Pos) -> Self {
        if end < start { Self { file, start: end, end: start } } else { Self { file, start, end } }
    }

    /// The bytes the span covers, lowest offset first even if the fields were set inverted.
    #[must_use]
    pub const fn byte_range(&self) -> Range<usize> {
        let (a, b) = (self.start.offset, self.end.offset);
        if a <= b { a..b } else { b..a }
    }
}

#[cfg(test)]
mod tests {
    use super::{Pos, SourceSpan};
    use crate::SourceFileId;

    fn span(file: u32, line: u32, column: u32) -> SourceSpan {
        let start = Pos::new(line, column, 0);
        SourceSpan::new(SourceFileId::new(file), start, Pos::new(line, column + 1, 1))
    }

    #[test]
    fn spans_order_by_file_then_position() {
        let mut spans = [span(1, 1, 1), span(0, 3, 1), span(0, 2, 9), span(0, 2, 4)];
        spans.sort();
        let order: Vec<_> = spans.iter().map(|s| (s.file.index(), s.start.line, s.start.column)).collect();
        assert_eq!(order, [(0, 2, 4), (0, 2, 9), (0, 3, 1), (1, 1, 1)]);
    }

    #[test]
    fn byte_range_runs_from_start_to_end_offset() {
        let span = SourceSpan::new(SourceFileId::new(0), Pos::new(2, 3, 14), Pos::new(2, 8, 19));
        assert_eq!(span.byte_range(), 14..19);
        assert_eq!(Pos::START, Pos::new(1, 1, 0));
    }

    #[test]
    fn inverted_spans_are_normalised() {
        let (early, late) = (Pos::new(2, 3, 14), Pos::new(2, 8, 19));
        let span = SourceSpan::new(SourceFileId::new(0), late, early);
        assert_eq!((span.start, span.end), (early, late));
        let hand_built = SourceSpan { file: SourceFileId::new(0), start: late, end: early };
        assert_eq!(hand_built.byte_range(), 14..19);
    }
}
