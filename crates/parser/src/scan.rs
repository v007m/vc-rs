//! Hand-written scanning: a cursor that keeps positions, and the problem that stops it.

use vc_model::{Pos, SourceFileId, SourceSpan};

/// A place in the text: the byte offset, the line and the column in characters. Only `\n`
/// starts a line, so positions agree with [`vc_model::SourceFiles::position`].
pub struct Cursor<'t> {
    text: &'t str,
    file: SourceFileId,
    pos: Pos,
}

/// What stopped a scan: where, what went wrong and how to fix it.
pub struct Fault {
    /// The characters at fault, or an empty span at the end of the text.
    pub at: SourceSpan,
    /// What is wrong.
    pub message: String,
    /// How to fix it.
    pub help: &'static str,
}

/// `text` for a message: as written, except that control characters are escaped.
pub fn shown(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { c.escape_debug().to_string() } else { c.to_string() }).collect()
}

const BOM: char = '\u{feff}';

impl<'t> Cursor<'t> {
    /// A cursor at the start of `text`, after a byte order mark if there is one.
    pub fn new(file: SourceFileId, text: &'t str) -> Self {
        let offset = if text.starts_with(BOM) { BOM.len_utf8() } else { 0 };
        Self { text, file, pos: Pos::new(1, 1, offset) }
    }

    /// Where the cursor is.
    pub const fn pos(&self) -> Pos {
        self.pos
    }

    /// The text from the cursor on.
    pub fn rest(&self) -> &'t str {
        self.text.get(self.pos.offset..).unwrap_or_default()
    }

    /// The character at the cursor.
    pub fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    /// Moves past one character.
    pub fn bump(&mut self) {
        if let Some(c) = self.peek() {
            self.pos = if c == '\n' {
                Pos::new(self.pos.line.saturating_add(1), 1, self.pos.offset + 1)
            } else {
                Pos::new(self.pos.line, self.pos.column.saturating_add(1), self.pos.offset + c.len_utf8())
            };
        }
    }

    /// Moves past `c` if it is next.
    pub fn eat(&mut self, c: char) -> bool {
        let found = self.peek() == Some(c);
        if found {
            self.bump();
        }
        found
    }

    /// Moves past `count` characters that the caller knows are ASCII and not line breaks.
    pub fn skip_ascii(&mut self, count: usize) {
        let columns = u32::try_from(count).unwrap_or(u32::MAX);
        let offset = (self.pos.offset + count).min(self.text.len());
        self.pos = Pos::new(self.pos.line, self.pos.column.saturating_add(columns), offset);
    }

    /// Moves past spaces, tabs and line breaks.
    pub fn skip_blanks(&mut self) {
        while self.peek().is_some_and(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) {
            self.bump();
        }
    }

    /// The span from `start` to `end`.
    pub fn span(&self, start: Pos, end: Pos) -> SourceSpan {
        SourceSpan::new(self.file, start, end)
    }

    /// The span from `start` to the cursor.
    pub fn span_from(&self, start: Pos) -> SourceSpan {
        self.span(start, self.pos)
    }

    /// The span of the character at the cursor, or an empty span at the end of the text.
    pub fn here(&self) -> SourceSpan {
        let mut next = Self { text: self.text, file: self.file, pos: self.pos };
        next.bump();
        self.span(self.pos, next.pos)
    }

    /// The text between two positions.
    pub fn slice(&self, start: Pos, end: Pos) -> &'t str {
        self.text.get(start.offset..end.offset).unwrap_or_default()
    }
}
