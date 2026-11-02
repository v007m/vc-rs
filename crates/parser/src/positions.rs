use saphyr_parser::{Marker, Span};
use vc_model::{Pos, SourceFileId, SourceSpan};

/// Converts saphyr's markers (character index, 1-based line, 0-based character column) into
/// [`Pos`] values (byte offset, 1-based line, 1-based character column).
pub struct Positions {
    file: SourceFileId,
    /// The byte offset of each character, plus the text length; empty for ASCII text, where
    /// character and byte offsets are the same.
    offsets: Vec<usize>,
    len: usize,
}

impl Positions {
    /// The table for `text` in `file`.
    pub fn new(file: SourceFileId, text: &str) -> Self {
        let offsets = if text.is_ascii() {
            Vec::new()
        } else {
            text.char_indices().map(|(offset, _)| offset).chain([text.len()]).collect()
        };
        Self { file, offsets, len: text.len() }
    }

    /// The file spans are made for.
    pub const fn file(&self) -> SourceFileId {
        self.file
    }

    fn offset(&self, index: usize) -> usize {
        if self.offsets.is_empty() { index.min(self.len) } else { self.offsets.get(index).copied().unwrap_or(self.len) }
    }

    /// The position of `marker`.
    pub fn pos(&self, marker: &Marker) -> Pos {
        let line = u32::try_from(marker.line()).unwrap_or(u32::MAX);
        let column = u32::try_from(marker.col().saturating_add(1)).unwrap_or(u32::MAX);
        Pos::new(line, column, self.offset(marker.index()))
    }

    /// The span saphyr's `span` covers.
    pub fn span(&self, span: &Span) -> SourceSpan {
        SourceSpan::new(self.file, self.pos(&span.start), self.pos(&span.end))
    }

    /// The span of the one character at `marker`, or an empty span at the end of the text.
    pub fn char_at(&self, marker: &Marker) -> SourceSpan {
        let start = self.pos(marker);
        let next = Marker::new(marker.index().saturating_add(1), marker.line(), marker.col().saturating_add(1));
        let end = if start.offset < self.len { self.pos(&next) } else { start };
        SourceSpan::new(self.file, start, end)
    }
}
