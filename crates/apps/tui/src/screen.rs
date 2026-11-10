//! What a tab is: a screen with its own keys, the updates it takes and how it draws. The tabs
//! are a registry of screens (`tabs.rs`), so the shell never matches on which tab is chosen.

use std::fmt::Debug;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use vc_engine::{RunFeed, RunRequest, RunRow, TestRow, TestTrend};

/// Work a key asks for that needs the engine; [`crate::run`] carries it out and sends back
/// [`Update`]s.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ask {
    /// Start this run, watched; its feed comes back as [`Update::Run`].
    Run(Box<RunRequest>),
    /// Interrupt the run in progress, as one Ctrl-C does to `vc run`.
    Stop,
    /// Read the newest stored runs and the tests' trends over them: [`Update::History`].
    History,
    /// Read the tests of two stored runs: [`Update::Compared`].
    Compare {
        /// The earlier run's number.
        before: u64,
        /// The later run's number.
        after: u64,
    },
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
    /// Applies a key the shell did not take; what it asks of the engine, if anything.
    fn press(&mut self, key: KeyEvent) -> Option<Ask>;
    /// Takes an update meant for this screen and ignores the rest.
    fn take(&mut self, update: &Update);
    /// Draws the screen into `area`.
    fn draw(&self, frame: &mut Frame<'_>, area: Rect);
}
