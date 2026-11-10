//! The History tab: the stored runs, newest first, with the pass rate and duration trends,
//! the tests that keep failing or flaking, and the chosen run compared with the one before.
//! It reads the history when first shown, and again on `r` or when shown after a run.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use vc_engine::{RunFeed, RunRow, TestTrend};

use crate::history_model::Comparison;
use crate::history_view;
use crate::screen::{Ask, Screen, Update, cursor_move};
use crate::theme::clean;

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
    stale: bool,
    runs: Vec<RunRow>,
    trends: Vec<TestTrend>,
    cursor: usize,
    comparing: Option<(u64, u64)>,
    compared: Option<Comparison>,
}

impl HistoryScreen {
    /// Where reading the history has got to.
    pub const fn reading(&self) -> &ReadState {
        &self.reading
    }

    /// Whether a run has finished since the history was read.
    pub const fn stale(&self) -> bool {
        self.stale
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

    /// Asks for the history again, unless it is being read.
    fn reload(&mut self) -> Option<Ask> {
        if self.reading == ReadState::Reading {
            return None;
        }
        self.reading = ReadState::Reading;
        Some(Ask::History)
    }

    /// Asks for the chosen run's tests and those of the run before it.
    fn compare(&mut self) -> Option<Ask> {
        let after = self.runs.get(self.cursor)?.id.get();
        let before = self.runs.get(self.cursor + 1)?.id.get();
        self.comparing = Some((before, after));
        Some(Ask::Compare { before, after })
    }

    fn step(&mut self, down: bool) {
        let last = self.runs.len().saturating_sub(1);
        self.cursor = if down { (self.cursor + 1).min(last) } else { self.cursor.saturating_sub(1) };
    }

    fn read(&mut self, runs: &[RunRow], trends: &[TestTrend]) {
        self.runs = runs.to_vec();
        self.trends = trends.to_vec();
        self.reading = ReadState::Read;
        self.stale = false;
        self.cursor = self.cursor.min(self.runs.len().saturating_sub(1));
    }

    fn failed(&mut self, reason: &str) {
        self.reading = ReadState::Failed(clean(reason.lines().next().unwrap_or_default()));
        self.comparing = None;
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
        if self.reading == ReadState::Unread || self.stale { self.reload() } else { None }
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
            Update::Run(RunFeed::Done(_)) | Update::Loaded(_) => self.stale = true,
            Update::History { runs, trends } => self.read(runs, trends),
            Update::Compared { before, after } if self.comparing == Some((before.0, after.0)) => {
                self.compared = Some(Comparison::new((before.0, after.0), &before.1, &after.1));
                self.comparing = None;
            }
            Update::HistoryFailed(reason) => self.failed(reason),
            _ => {}
        }
    }

    fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        history_view::draw(self, frame, area);
    }
}
