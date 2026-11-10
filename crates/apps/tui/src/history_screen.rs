//! The History tab: the stored runs, newest first, with the pass rate and duration trends,
//! the tests that keep failing or flaking, and the chosen run compared with the one before.
//! It reads the history when first shown, and again on `r` or when shown after a run.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use vc_engine::{RunFeed, RunRow, TestRow, TestTrend};

use crate::history_model::{self, Comparison};
use crate::history_view;
use crate::screen::{Ask, Screen, Update, clamp_step, cursor_move};
use crate::theme::first_line;

/// The History tab's keys.
const KEYS: [(&str, &str); 3] = [("↑↓", "move"), ("enter", "compare"), ("r", "reload")];

/// Where reading the history has got to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ReadState {
    /// Not asked for yet.
    #[default]
    Unread,
    /// Asked for.
    Reading,
    /// Read.
    Read,
    /// The history could not be read: the first line of why.
    Failed(String),
}

/// The History tab.
#[derive(Debug, Default)]
pub struct HistoryScreen {
    reading: ReadState,
    finished: u64,
    asked: u64,
    seen: u64,
    runs: Vec<RunRow>,
    trends: Vec<TestTrend>,
    cursor: usize,
    comparing: Option<(u64, u64)>,
    compared: Option<Comparison>,
    compare_error: Option<String>,
}

impl HistoryScreen {
    /// Where reading the history has got to.
    pub const fn reading(&self) -> &ReadState {
        &self.reading
    }

    /// Whether a run has finished since the history shown was asked for. Runs are counted as
    /// they finish: `asked` is the count when the last read was asked for, and `seen` the
    /// count the history shown was asked for at.
    pub const fn stale(&self) -> bool {
        self.finished > self.seen
    }

    /// The stored runs, newest first.
    pub fn runs(&self) -> &[RunRow] {
        &self.runs
    }

    /// Each test's results over the newest runs.
    pub fn trends(&self) -> &[TestTrend] {
        &self.trends
    }

    /// The chosen run's position.
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// The two runs being read for a comparison.
    pub const fn comparing(&self) -> Option<(u64, u64)> {
        self.comparing
    }

    /// The last comparison read.
    pub const fn compared(&self) -> Option<&Comparison> {
        self.compared.as_ref()
    }

    /// Why the last comparison could not be read: the first line.
    pub fn compare_error(&self) -> Option<&str> {
        self.compare_error.as_deref()
    }

    /// Asks for the history again, unless it is being read.
    fn reload(&mut self) -> Option<Ask> {
        if self.reading == ReadState::Reading {
            return None;
        }
        self.reading = ReadState::Reading;
        self.asked = self.finished;
        Some(Ask::History)
    }

    /// Asks for the chosen run's tests and those of the run of its kind before it.
    fn compare(&mut self) -> Option<Ask> {
        let after = self.runs.get(self.cursor)?.id.get();
        let before = history_model::earlier(&self.runs, self.cursor)?.id.get();
        self.comparing = Some((before, after));
        self.compare_error = None;
        Some(Ask::Compare { before, after })
    }

    fn step(&mut self, down: bool) {
        self.cursor = clamp_step(self.cursor, self.runs.len(), down);
    }

    fn read(&mut self, runs: &[RunRow], trends: &[TestTrend]) {
        self.runs = runs.to_vec();
        self.trends = trends.to_vec();
        self.reading = ReadState::Read;
        self.seen = self.asked;
        self.cursor = self.cursor.min(self.runs.len().saturating_sub(1));
    }

    /// Takes the two runs' tests, if they are the ones asked for.
    fn compared_runs(&mut self, before: &(u64, Vec<TestRow>), after: &(u64, Vec<TestRow>)) {
        if self.comparing == Some((before.0, after.0)) {
            self.compared = Some(Comparison::new((before.0, after.0), &before.1, &after.1));
            self.comparing = None;
        }
    }

    /// Keeps why the comparison asked for could not be read, on its pane: the runs listed
    /// stay as they are.
    fn compare_failed(&mut self, reason: &str) {
        if self.comparing.take().is_some() {
            self.compare_error = Some(first_line(reason));
        }
    }
}

impl Screen for HistoryScreen {
    fn title(&self) -> &'static str {
        "History"
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &KEYS
    }

    fn shown(&mut self) -> Option<Ask> {
        if self.reading == ReadState::Unread || self.stale() { self.reload() } else { None }
    }

    fn press(&mut self, key: KeyEvent) -> Option<Ask> {
        if let Some(down) = cursor_move(key.code) {
            self.step(down);
            return None;
        }
        match key.code {
            KeyCode::Enter => return self.compare(),
            KeyCode::Char('r') => return self.reload(),
            _ => {}
        }
        None
    }

    fn take(&mut self, update: &Update) {
        match update {
            Update::Run(RunFeed::Done(_)) | Update::Loaded(_) => self.finished = self.finished.saturating_add(1),
            Update::History { runs, trends } => self.read(runs, trends),
            Update::Compared { before, after } => self.compared_runs(before, after),
            Update::HistoryFailed(reason) => self.reading = ReadState::Failed(first_line(reason)),
            Update::CompareFailed(reason) => self.compare_failed(reason),
            _ => {}
        }
    }

    fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        history_view::draw(self, frame, area);
    }
}
