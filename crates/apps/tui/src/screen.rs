//! What a tab is: a screen with its own keys, the updates it takes and how it draws. The tabs
//! are a registry of screens (`tabs.rs`), so the shell never matches on which tab is chosen.

use std::fmt::Debug;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use vc_engine::{
    Changes, Event, ImportRequest, Listed, LoadReport, LoadRequest, RunFeed, RunRequest, RunRow, SkipRequest, TestRow,
    TestTrend,
};

/// Work a key asks for that needs the engine; [`crate::run`] carries it out and sends back
/// [`Update`]s.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ask {
    /// Start this run, watched; its feed comes back as [`Update::Run`].
    Run(Box<RunRequest>),
    /// Interrupt the run or load run in progress, when it is of this kind, as one Ctrl-C does
    /// to `vc run`; asked again, end it outright, its cleanups abandoned.
    Stop(Flight),
    /// Read the newest stored runs and the tests' trends over them: [`Update::History`].
    History,
    /// Read the tests of two stored runs: [`Update::Compared`].
    Compare {
        /// The earlier run's number.
        before: u64,
        /// The later run's number.
        after: u64,
    },
    /// Read the project's apis and tests: [`Update::Explored`].
    Explore,
    /// Explain a test or api as a run under `profiles` sees it: [`Update::Explained`].
    Explain {
        /// Its full ID.
        id: String,
        /// The active profiles.
        profiles: Vec<String>,
    },
    /// Skip a test with a reason, write the change and read the project again.
    Skip(SkipRequest),
    /// Remove a test's skip, write the change and read the project again.
    Unskip(String),
    /// List the project's load scenarios: [`Update::Scenarios`].
    Scenarios,
    /// Start this load run, its seconds watched: [`Update::Load`], then [`Update::Loaded`].
    /// [`Ask::Stop`] interrupts it.
    Load(LoadRequest),
    /// Draft this import, writing nothing: [`Update::Drafted`].
    Import(Box<ImportRequest>),
    /// Write a drafted import: [`Update::Written`].
    Write(Changes),
}

/// The work in flight that [`Ask::Stop`] stops and quitting ends: a run or a load run, one at
/// a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Flight {
    /// A run, on the Run tab.
    Run,
    /// A load run, on the Load tab.
    Load,
}

/// What the engine sent back for an [`Ask`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Update {
    /// The next item of the watched run's feed.
    Run(RunFeed),
    /// The run could not start or ended in an error; the error's text.
    RunFailed(String),
    /// The newest stored runs, newest first, and how each test did over them.
    History {
        /// The runs.
        runs: Vec<RunRow>,
        /// Each test's results over the runs of kind `run`, in test ID order.
        trends: Vec<TestTrend>,
    },
    /// The tests of the two runs an [`Ask::Compare`] named.
    Compared {
        /// The earlier run's number and its tests.
        before: (u64, Vec<TestRow>),
        /// The later run's number and its tests.
        after: (u64, Vec<TestRow>),
    },
    /// The history could not be read; the error's text.
    HistoryFailed(String),
    /// The runs an [`Ask::Compare`] named could not be read.
    CompareFailed {
        /// The earlier run's number, as asked.
        before: u64,
        /// The later run's number, as asked.
        after: u64,
        /// The error's text.
        reason: String,
    },
    /// The project's apis, then its tests, each in project order.
    Explored {
        /// The apis and tests.
        items: Vec<Listed>,
        /// Problems that did not stop the reading, one line each.
        notices: Vec<String>,
    },
    /// How a run sees the test or api an [`Ask::Explain`] named.
    Explained {
        /// Its full ID.
        id: String,
        /// The explanation, one item per line.
        text: String,
    },
    /// A skip was written (`skipped`) or removed for `target`.
    Quarantined {
        /// The test.
        target: String,
        /// Whether it is skipped now.
        skipped: bool,
    },
    /// The project could not be read; the error's text.
    ExploreFailed(String),
    /// The test or api an [`Ask::Explain`] named could not be explained.
    ExplainFailed {
        /// Its full ID, as asked.
        id: String,
        /// The error's text.
        reason: String,
    },
    /// A skip or unskip could not be made or written; the error's text.
    SkipFailed(String),
    /// The project's load files, in project order.
    Scenarios(Vec<Listed>),
    /// The scenarios could not be listed; the error's text. A load run in progress goes on.
    ScenariosFailed(String),
    /// One closed second of the load run: an [`Event::LoadTick`].
    Load(Event),
    /// The load run is over.
    Loaded(Box<LoadReport>),
    /// The load run could not start or ended in an error; the error's text.
    LoadFailed(String),
    /// The files the import would create or change, not written.
    Drafted(Changes),
    /// The drafted files were written; the change's titles.
    Written(Vec<String>),
    /// The import was refused, or could not be written; the error's text and its help.
    ImportFailed(String),
}

/// Where a cursor key moves a list's cursor: down for `↓` or `j`, up for `↑` or `k`.
pub const fn cursor_move(code: KeyCode) -> Option<bool> {
    match code {
        KeyCode::Down | KeyCode::Char('j') => Some(true),
        KeyCode::Up | KeyCode::Char('k') => Some(false),
        _ => None,
    }
}

/// The lines `pgdn` and `pgup` scroll a pane by.
const PAGE: u16 = 10;

/// A pane's scroll after `code`, when it is `pgdn` or `pgup`: a page on or back, kept within
/// the `lines` the pane holds.
pub fn scrolled(scroll: u16, code: KeyCode, lines: usize) -> Option<u16> {
    let last = u16::try_from(lines.saturating_sub(1)).unwrap_or(u16::MAX);
    match code {
        KeyCode::PageDown => Some(scroll.saturating_add(PAGE).min(last)),
        KeyCode::PageUp => Some(scroll.saturating_sub(PAGE)),
        _ => None,
    }
}

/// The cursor one row down or up a list of `len` rows, kept on the list.
pub fn clamp_step(cursor: usize, len: usize, down: bool) -> usize {
    if down { (cursor + 1).min(len.saturating_sub(1)) } else { cursor.saturating_sub(1) }
}

/// The cursor on a list read again: on the row `chosen` finds, wherever the new read put it,
/// otherwise `cursor` kept on the list.
pub fn refound<T>(rows: &[T], chosen: impl Fn(&T) -> bool, cursor: usize) -> usize {
    rows.iter().position(chosen).unwrap_or_else(|| cursor.min(rows.len().saturating_sub(1)))
}

/// One tab's screen.
pub trait Screen: Debug + Send {
    /// The title, in the tab bar and on the pane.
    fn title(&self) -> &'static str;
    /// The tab's own keys, for the footer: a key and what it does.
    fn keys(&self) -> &'static [(&'static str, &'static str)];
    /// The tab was chosen; what it asks of the engine to show, if anything.
    fn shown(&mut self) -> Option<Ask> {
        None
    }
    /// Whether the screen is taking text, so the shell hands it every key but `ctrl-c`.
    fn typing(&self) -> bool {
        false
    }
    /// Applies a key the shell did not take; what it asks of the engine, if anything.
    fn press(&mut self, key: KeyEvent) -> Option<Ask>;
    /// Takes an update meant for this screen and ignores the rest.
    fn take(&mut self, update: &Update);
    /// Draws the screen into `area`.
    fn draw(&self, frame: &mut Frame<'_>, area: Rect);
}
