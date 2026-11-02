use std::ops::Range;

use vc_model::{Pos, SourceSpan};

/// Maps byte ranges of a scalar's decoded text to spans in its source file.
///
/// Exact when the scalar sits on one line and its source is the text itself (a plain scalar)
/// or the text between two quotes (a quoted scalar without escapes). Otherwise, such as a
/// block scalar, a folded line or a quoted scalar with escapes, every range maps to the whole
/// scalar: the diagnostic points at the right value, not the right character.
pub struct SpanMap<'a> {
    text: &'a str,
    scalar: SourceSpan,
    shift: Option<usize>,
}

impl<'a> SpanMap<'a> {
    /// The map for `text`, read from the scalar at `scalar`.
    pub fn new(text: &'a str, scalar: SourceSpan) -> Self {
        let source_len = scalar.byte_range().len();
        let one_line = scalar.start.line == scalar.end.line;
        let shift = [0, 1].into_iter().find(|quotes| one_line && source_len == text.len() + 2 * quotes);
        Self { text, scalar, shift }
    }

    /// The source span of `range`, a byte range of the text on char boundaries.
    pub fn span(&self, range: Range<usize>) -> SourceSpan {
        self.shift.map_or(self.scalar, |shift| {
            SourceSpan::new(self.scalar.file, self.pos(range.start, shift), self.pos(range.end, shift))
        })
    }

    /// The span of the character at `offset`, or an empty span at the end of the text.
    pub fn char_span(&self, offset: usize) -> SourceSpan {
        let width = self.text.get(offset..).and_then(|rest| rest.chars().next()).map_or(0, char::len_utf8);
        self.span(offset..offset + width)
    }

    fn pos(&self, offset: usize, shift: usize) -> Pos {
        let chars = self.text.get(..offset).map_or(0, |before| before.chars().count()) + shift;
        let column = self.scalar.start.column.saturating_add(u32::try_from(chars).unwrap_or(u32::MAX));
        Pos::new(self.scalar.start.line, column, self.scalar.start.offset + offset + shift)
    }
}
