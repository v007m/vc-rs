//! What a tab is: a screen with its own keys, the updates it takes and how it draws. The tabs
//! are a registry of screens (`tabs.rs`), so the shell never matches on which tab is chosen.

use std::fmt::Debug;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use vc_engine::{
    Event, Listed, LoadReport, LoadRequest, RunFeed, RunRequest, RunRow, SkipRequest, TestRow, TestTrend,
};

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
    /// The project's apis, then its tests, each in project order.
    Explored(Vec<Listed>),
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
    /// The project could not be read, explained or changed; the error's text.
    ExploreFailed(String),
    /// The project's load files, in project order.
    Scenarios(Vec<Listed>),
    /// One closed second of the load run: an [`Event::LoadTick`].
    Load(Event),
    /// The load run is over.
    Loaded(Box<LoadReport>),
    /// The scenarios could not be listed, or the load run could not start or ended in an
    /// error; the error's text.
    LoadFailed(String),
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
