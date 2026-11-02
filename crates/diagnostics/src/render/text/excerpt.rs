use std::iter;

use vc_model::{Pos, SourceFileId, SourceSpan};

use crate::{Diagnostic, Label};

/// Spans covering more lines than this show their first two lines, `...`, and their last.
const MAX_FULL_LINES: u32 = 4;

/// One label to draw: its span, trimmed by [`trimmed`], and its text.
pub(super) struct Mark<'a> {
    pub(super) span: SourceSpan,
    pub(super) text: &'a str,
    pub(super) primary: bool,
}

/// How to underline one line of a mark: `pad` before the carets, `width` carets, then `text`.
pub(super) struct Underline<'a> {
    pub(super) pad: String,
    pub(super) width: usize,
    pub(super) text: Option<&'a str>,
}

impl<'a> Mark<'a> {
    fn new(label: &'a Label, primary: bool) -> Self {
        Self { span: trimmed(label.span), text: &label.text, primary }
    }

    fn lines(&self) -> Vec<u32> {
        let (first, last) = (self.span.start.line, self.span.end.line);
        if last - first < MAX_FULL_LINES { (first..=last).collect() } else { vec![first, first + 1, last] }
    }

    fn covers(&self, line: u32) -> bool {
        (self.span.start.line..=self.span.end.line).contains(&line)
    }

    /// The underline under `line`, whose source is `text`.
    ///
    /// The span's first line is underlined from its start column, its last line up to its end
    /// column, and lines between from their first non-blank character to their end. The label
    /// text goes on the last line only.
    pub(super) fn underline(&self, line: u32, text: &str) -> Underline<'a> {
        let past_end = text.chars().count() + 1;
        let first_visible = text.chars().take_while(|c| c.is_whitespace()).count() + 1;
        let from = if line == self.span.start.line { column(self.span.start.column) } else { first_visible };
        let to = if line == self.span.end.line { column(self.span.end.column) } else { past_end };
        let (from, to) = (from.clamp(1, past_end), to.clamp(1, past_end));
        let pad = text.chars().take(from - 1).map(|c| if c == '\t' { '\t' } else { ' ' }).collect();
        let labelled = line == self.span.end.line && !self.text.is_empty();
        Underline { pad, width: to.saturating_sub(from).max(1), text: labelled.then_some(self.text) }
    }
}

/// A run of lines from one file, with every mark that falls on them.
///
/// Marks in the same file whose lines overlap or touch share an excerpt, as in rustc. The
/// `anchor` is the first mark's span, which the excerpt's location line points at.
pub(super) struct Excerpt<'a> {
    pub(super) file: SourceFileId,
    pub(super) anchor: SourceSpan,
    first: u32,
    last: u32,
    marks: Vec<Mark<'a>>,
}

impl<'a> Excerpt<'a> {
    fn new(mark: Mark<'a>) -> Self {
        let (first, last) = (mark.span.start.line, mark.span.end.line);
        Self { file: mark.span.file, anchor: mark.span, first, last, marks: vec![mark] }
    }

    fn touches(&self, mark: &Mark<'_>) -> bool {
        mark.span.file == self.file
            && mark.span.start.line <= self.last.saturating_add(1)
            && mark.span.end.line.saturating_add(1) >= self.first
    }

    fn add(&mut self, mark: Mark<'a>) {
        self.first = self.first.min(mark.span.start.line);
        self.last = self.last.max(mark.span.end.line);
        self.marks.push(mark);
    }

    /// The lines to show, in order; `None` stands for elided lines.
    pub(super) fn shown_lines(&self) -> Vec<Option<u32>> {
        let mut lines: Vec<u32> = self.marks.iter().flat_map(Mark::lines).collect();
        lines.sort_unstable();
        lines.dedup();
        let mut shown = Vec::with_capacity(lines.len() + 1);
        let mut previous: Option<u32> = None;
        for line in lines {
            if previous.is_some_and(|before| line > before + 1) {
                shown.push(None);
            }
            shown.push(Some(line));
            previous = Some(line);
        }
        shown
    }

    /// The marks covering `line`, primary first, then in label order.
    pub(super) fn marks_on(&self, line: u32) -> impl Iterator<Item = &Mark<'a>> {
        self.marks.iter().filter(move |mark| mark.covers(line))
    }
}

/// The excerpts of `diagnostic`: the primary label's first, then one per run of secondaries
/// that does not touch an earlier excerpt.
pub(super) fn excerpts(diagnostic: &Diagnostic) -> Vec<Excerpt<'_>> {
    let secondary = diagnostic.secondary.iter().map(|label| Mark::new(label, false));
    let mut excerpts: Vec<Excerpt<'_>> = Vec::new();
    for mark in iter::once(Mark::new(&diagnostic.primary, true)).chain(secondary) {
        match excerpts.iter_mut().find(|excerpt| excerpt.touches(&mark)) {
            Some(excerpt) => excerpt.add(mark),
            None => excerpts.push(Excerpt::new(mark)),
        }
    }
    excerpts
}

/// The width of the widest line number any excerpt shows.
pub(super) fn gutter_width(excerpts: &[Excerpt<'_>]) -> usize {
    excerpts.iter().map(|excerpt| excerpt.last).max().unwrap_or(1).to_string().len()
}

/// `span` with its end on or after its start line, and an end at column 1 of a later line
/// (just past a newline) moved to the end of the line before, so that line is not shown.
fn trimmed(span: SourceSpan) -> SourceSpan {
    let line = span.end.line.max(span.start.line);
    let end = if line > span.start.line && span.end.column <= 1 {
        Pos { line: line - 1, column: u32::MAX, ..span.end }
    } else {
        Pos { line, ..span.end }
    };
    SourceSpan { end, ..span }
}

fn column(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}
