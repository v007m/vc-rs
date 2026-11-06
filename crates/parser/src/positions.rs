use saphyr_parser::{Marker, Span};
use vc_model::{Pos, SourceFileId, SourceSpan};

/// Converts saphyr's markers (character index, 1-based line, 0-based character column) into
/// [`Pos`] values (byte offset, 1-based line, 1-based character column).
pub struct Positions {
    file: SourceFileId,
    /// One entry per non-ASCII character: its character index and the extra bytes (beyond one
    /// per character) of every character up to and including it. Empty for ASCII text.
    wide: Vec<(usize, usize)>,
    len: usize,
}

impl Positions {
    /// The table for `text` in `file`.
    pub fn new(file: SourceFileId, text: &str) -> Self {
        let mut extra = 0;
        let wide = text
            .chars()
            .enumerate()
            .filter(|(_, c)| !c.is_ascii())
            .map(|(index, c)| {
                extra += c.len_utf8() - 1;
                (index, extra)
            })
            .collect();
        Self { file, wide, len: text.len() }
    }

    fn offset(&self, index: usize) -> usize {
        let before = self.wide.partition_point(|&(at, _)| at < index);
        let extra = before.checked_sub(1).and_then(|last| self.wide.get(last)).map_or(0, |&(_, extra)| extra);
        index.saturating_add(extra).min(self.len)
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

/// `span` without the `&anchor` and `!tag` properties (and the blanks after them) that saphyr
/// counts as part of a node, so the span starts at the value itself.
pub fn without_properties(text: &str, span: SourceSpan) -> SourceSpan {
    let mut start = span.start;
    let mut rest = text.get(start.offset..).unwrap_or_default();
    while rest.starts_with(['&', '!']) {
        let word = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let after = rest.get(word..).unwrap_or_default();
        let skip = word + (after.len() - after.trim_start().len());
        start = advance(start, rest.get(..skip).unwrap_or_default());
        rest = rest.get(skip..).unwrap_or_default();
    }
    let end = if span.end.offset < start.offset { start } else { span.end };
    SourceSpan::new(span.file, start, end)
}

/// The position just after `skipped`, which starts at `from`.
fn advance(from: Pos, skipped: &str) -> Pos {
    skipped.chars().fold(from, |pos, c| {
        let offset = pos.offset + c.len_utf8();
        if c == '\n' { Pos::new(pos.line + 1, 1, offset) } else { Pos::new(pos.line, pos.column + 1, offset) }
    })
}

/// `span` cut back to the closing quote when it holds a quoted scalar; the scanner's span of
/// one runs on over the spaces and comment after it.
pub fn to_closing_quote(text: &str, span: SourceSpan) -> SourceSpan {
    let rest = text.get(span.start.offset..span.end.offset).unwrap_or_default();
    let Some(quote) = rest.chars().next().filter(|c| matches!(c, '\'' | '"')) else {
        return span;
    };
    let mut escaped = false;
    let mut chars = rest.char_indices().skip(1).peekable();
    while let Some((at, c)) = chars.next() {
        if quote == '"' && (escaped || c == '\\') {
            escaped = !escaped;
        } else if c == quote && quote == '\'' && chars.peek().is_some_and(|&(_, next)| next == '\'') {
            chars.next();
        } else if c == quote {
            let quoted = rest.get(..=at).unwrap_or_default();
            return SourceSpan::new(span.file, span.start, advance(span.start, quoted));
        }
    }
    span
}
