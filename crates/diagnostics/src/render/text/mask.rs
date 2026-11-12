use std::borrow::Cow;

use vc_model::{SourceFileId, SourceSpan};

/// What a masked run of characters prints as, whatever its length, so the excerpt shows
/// neither the text nor how long it is.
const DOTS: &str = "•••";
const DOTS_WIDTH: usize = 3;

/// The masked runs of one source line, as 1-based character columns, end exclusive, sorted
/// and not overlapping.
pub(super) struct LineMasks(Vec<(usize, usize)>);

impl LineMasks {
    /// The runs of `text`, line `number` of `file`, that `masks` cover.
    pub(super) fn of(masks: &[SourceSpan], (file, number): (SourceFileId, u32), text: &str) -> Self {
        let past_end = text.chars().count() + 1;
        let on_line =
            masks.iter().filter(|mask| mask.file == file && (mask.start.line..=mask.end.line).contains(&number));
        let mut runs: Vec<(usize, usize)> = on_line
            .map(|mask| {
                let from = if mask.start.line == number { column(mask.start.column) } else { 1 };
                let to = if mask.end.line == number { column(mask.end.column) } else { past_end };
                (from.clamp(1, past_end), to.clamp(1, past_end))
            })
            .filter(|(from, to)| from < to)
            .collect();
        runs.sort_unstable();
        Self(merged(runs))
    }

    /// `text` with each run replaced by `•••`.
    pub(super) fn apply<'t>(&self, text: &'t str) -> Cow<'t, str> {
        if self.0.is_empty() {
            return Cow::Borrowed(text);
        }
        let mut out = String::with_capacity(text.len());
        for (at, c) in text.chars().enumerate().map(|(index, c)| (index + 1, c)) {
            match self.0.iter().find(|(from, to)| (*from..*to).contains(&at)) {
                Some(&(from, _)) if from == at => out.push_str(DOTS),
                Some(_) => {}
                None => out.push(c),
            }
        }
        Cow::Owned(out)
    }

    /// Where `value`, a column of the source line, falls on the masked line: a column after a
    /// run moves by the run's change in width, and one inside it stays inside the dots.
    pub(super) fn column(&self, value: u32) -> u32 {
        let at = column(value);
        let before = self.0.iter().filter(|(from, _)| *from < at);
        let (removed, added) = before.fold((0, 0), |(removed, added), &(from, to)| {
            let covered = to.min(at) - from;
            let shown = if at >= to { DOTS_WIDTH } else { covered.min(DOTS_WIDTH) };
            (removed + covered, added + shown)
        });
        u32::try_from(at.saturating_sub(removed).saturating_add(added)).unwrap_or(u32::MAX)
    }
}

/// `runs`, sorted by start, with overlapping or touching runs joined.
fn merged(runs: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let mut joined: Vec<(usize, usize)> = Vec::with_capacity(runs.len());
    for (from, to) in runs {
        match joined.last_mut() {
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => joined.push((from, to)),
        }
    }
    joined
}

fn column(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use vc_model::{Pos, SourceFileId, SourceSpan};

    use super::LineMasks;

    const FILE: SourceFileId = SourceFileId::new(0);

    fn at(line: u32, column: u32) -> Pos {
        Pos::new(line, column, 0)
    }

    fn masks(spans: &[SourceSpan], line: u32, text: &str) -> LineMasks {
        LineMasks::of(spans, (FILE, line), text)
    }

    #[test]
    fn a_run_of_any_length_prints_as_three_dots_and_later_columns_move_by_the_difference() {
        let long = SourceSpan::new(FILE, at(1, 4), at(1, 13));
        let short = SourceSpan::new(FILE, at(1, 14), at(1, 15));
        let line = masks(&[short, long], 1, "ab:sk-123456 x y");
        assert_eq!(line.apply("ab:sk-123456 x y"), "ab:••• ••• y");
        assert_eq!((line.column(1), line.column(4), line.column(13), line.column(15)), (1, 4, 7, 11));
        assert_eq!(line.column(6), 6, "a column inside a run stays inside the dots");
    }

    #[test]
    fn overlapping_runs_join_and_a_line_with_none_is_left_alone() {
        let first = SourceSpan::new(FILE, at(1, 2), at(1, 6));
        let second = SourceSpan::new(FILE, at(1, 4), at(1, 9));
        assert_eq!(masks(&[first, second], 1, "a1234567b").apply("a1234567b"), "a•••b");
        let elsewhere = SourceSpan { file: SourceFileId::new(1), ..first };
        assert_eq!(masks(&[elsewhere], 1, "a1234567b").apply("a1234567b"), "a1234567b");
        assert_eq!(masks(&[first], 2, "plain").column(9), 9);
    }

    #[test]
    fn a_span_over_several_lines_masks_each_of_them_up_to_its_end() {
        let block = SourceSpan::new(FILE, at(1, 7), at(3, 4));
        let text = ["token: abc", "  defgh", "  ij, kept"];
        let shown: Vec<_> =
            (1..).zip(text).map(|(line, text)| masks(&[block], line, text).apply(text).into_owned()).collect();
        assert_eq!(shown, ["token:•••", "•••", "•••j, kept"]);
    }
}
