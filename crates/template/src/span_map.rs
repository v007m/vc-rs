use std::ops::Range;

use vc_model::{Pos, ScalarSource, ScalarStyle, SourceSpan};

/// Maps byte ranges of a scalar's decoded text to spans in its source file.
///
/// Exact only when the scalar sits on one line and its source is the text itself: a plain
/// scalar, or a quoted one with no escapes (no `''` in single quotes, no backslash in double
/// quotes). Otherwise, such as a block scalar, a folded line or an escaped quoted scalar, every
/// range maps to the whole scalar: the diagnostic points at the right value, not the right
/// character, and offers no edit.
pub struct SpanMap<'a> {
    text: &'a str,
    scalar: SourceSpan,
    style: ScalarStyle,
    shift: Option<usize>,
}

impl<'a> SpanMap<'a> {
    /// The map for `text`, read from `source`.
    pub fn new(text: &'a str, source: ScalarSource) -> Self {
        let quotes = match source.style {
            ScalarStyle::Plain => Some(0),
            ScalarStyle::SingleQuoted { escaped: false } | ScalarStyle::DoubleQuoted { escaped: false } => Some(1),
            ScalarStyle::SingleQuoted { escaped: true }
            | ScalarStyle::DoubleQuoted { escaped: true }
            | ScalarStyle::Block => None,
        };
        let scalar = source.span;
        let one_line = scalar.start.line == scalar.end.line;
        let shift = quotes.filter(|quotes| one_line && scalar.byte_range().len() == text.len() + 2 * quotes);
        Self { text, scalar, style: source.style, shift }
    }

    /// The source span of `range`, a byte range of the text on char boundaries.
    pub fn span(&self, range: Range<usize>) -> SourceSpan {
        self.shift.map_or(self.scalar, |shift| {
            SourceSpan::new(self.scalar.file, self.pos(range.start, shift), self.pos(range.end, shift))
        })
    }

    /// Whether an edit may write a `'` at an exact span: the spans are exact and the scalar is
    /// not single-quoted, where a `'` would end it.
    pub const fn takes_single_quotes(&self) -> bool {
        self.shift.is_some() && !matches!(self.style, ScalarStyle::SingleQuoted { .. })
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
