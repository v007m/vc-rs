use std::iter;

use vc_model::{Pos, SourceFileId, SourceSpan};

use super::mask::LineMasks;
use crate::{Diagnostic, Label};

/// Spans covering more lines than this show their first two lines, `...`, and their last.
const MAX_FULL_LINES: u32 = 4;

/// One label to draw: its span, trimmed by [`trimmed`], its text, and its place among the
/// diagnostic's labels (the primary is 0).
#[derive(Clone, Copy)]
pub(super) struct Mark<'a> {
    pub(super) span: SourceSpan,
    pub(super) text: &'a str,
    pub(super) primary: bool,
    order: usize,
}

/// How to underline one line of a mark: `pad` before the carets, `width` carets, then `text`.
pub(super) struct Underline<'a> {
    pub(super) pad: String,
    pub(super) width: usize,
    pub(super) text: Option<&'a str>,
}

impl<'a> Mark<'a> {
    fn new(label: &'a Label, order: usize) -> Self {
        Self { span: trimmed(label.span), text: &label.text, primary: order == 0, order }
    }

    /// This mark on `line` once `masks` replace their runs, its columns there moved to match.
    pub(super) fn masked(&self, line: u32, masks: &LineMasks) -> Self {
        let mut span = self.span;
        if span.start.line == line {
            span.start.column = masks.column(span.start.column);
        }
        if span.end.line == line {
            span.end.column = masks.column(span.end.column);
        }
        Self { span, ..*self }
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

/// A run of lines from one file, with every mark that falls on them, primary first.
///
/// Marks whose lines overlap, touch or are one line apart share an excerpt, as in rustc.
pub(super) struct Excerpt<'a> {
    pub(super) file: SourceFileId,
    last: u32,
    marks: Vec<Mark<'a>>,
}

impl<'a> Excerpt<'a> {
    fn new(mark: Mark<'a>) -> Self {
        Self { file: mark.span.file, last: mark.span.end.line, marks: vec![mark] }
    }

    /// The lines to show, in order; `None` stands for two or more elided lines. A single
    /// missing line is shown rather than elided.
    pub(super) fn shown_lines(&self) -> Vec<Option<u32>> {
        let mut lines: Vec<u32> = self.marks.iter().flat_map(Mark::lines).collect();
        lines.sort_unstable();
        lines.dedup();
        let mut shown = Vec::with_capacity(lines.len() + 1);
        let mut previous: Option<u32> = None;
        for line in lines {
            match previous.map(|before| line - before) {
                Some(2) => shown.push(Some(line - 1)),
                Some(gap) if gap > 2 => shown.push(None),
                _ => {}
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

/// The excerpts of one file, by line, and the span its location line points at: the first of
/// the diagnostic's labels in that file.
pub(super) struct FileExcerpts<'a> {
    pub(super) anchor: SourceSpan,
    pub(super) excerpts: Vec<Excerpt<'a>>,
}

/// The excerpts of `diagnostic`, grouped by file: the primary label's file first, then the
/// others in the order their first label appears.
pub(super) fn excerpts(diagnostic: &Diagnostic) -> Vec<FileExcerpts<'_>> {
    let labels = iter::once(&diagnostic.primary).chain(&diagnostic.secondary);
    let mut files: Vec<(SourceSpan, Vec<Mark<'_>>)> = Vec::new();
    for mark in labels.enumerate().map(|(order, label)| Mark::new(label, order)) {
        match files.iter_mut().find(|(anchor, _)| anchor.file == mark.span.file) {
            Some((_, marks)) => marks.push(mark),
            None => files.push((mark.span, vec![mark])),
        }
    }
    files.into_iter().map(|(anchor, marks)| FileExcerpts { anchor, excerpts: sweep(marks) }).collect()
}

/// One file's marks swept in line order into excerpts, merging each into the one before when
/// it starts at most one line after that one ends.
fn sweep(mut marks: Vec<Mark<'_>>) -> Vec<Excerpt<'_>> {
    marks.sort_by_key(|mark| mark.span.start.line);
    let mut excerpts: Vec<Excerpt<'_>> = Vec::new();
    for mark in marks {
        match excerpts.last_mut() {
            Some(excerpt) if mark.span.start.line <= excerpt.last.saturating_add(2) => {
                excerpt.last = excerpt.last.max(mark.span.end.line);
                excerpt.marks.push(mark);
            }
            _ => excerpts.push(Excerpt::new(mark)),
        }
    }
    for excerpt in &mut excerpts {
        excerpt.marks.sort_by_key(|mark| mark.order);
    }
    excerpts
}

/// The width of the widest line number any excerpt shows.
pub(super) fn gutter_width(files: &[FileExcerpts<'_>]) -> usize {
    let lasts = files.iter().flat_map(|file| &file.excerpts).map(|excerpt| excerpt.last);
    lasts.max().unwrap_or(1).to_string().len()
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
