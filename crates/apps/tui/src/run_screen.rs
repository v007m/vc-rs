//! The Run tab: starts a watched run and shows it live, the tests by file on the left, the
//! chosen test's checks and the latest requests on the right.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use vc_engine::{PlanRequest, RunFeed, RunRequest, SelectRequest, TestStatus};

use crate::run_model::{Finished, RunModel};
use crate::run_view;
use crate::screen::{Ask, Flight, Screen, Update, clamp_step, cursor_move};

/// The Run tab's keys.
const KEYS: [(&str, &str); 6] =
    [("↑↓", "move"), ("a", "run all"), ("r", "rerun test"), ("R", "rerun failed"), ("f", "failures"), ("x", "stop")];
/// The Run tab's keys once `x` asked the run to stop.
const KEYS_STOPPING: [(&str, &str); 6] = [
    ("↑↓", "move"),
    ("a", "run all"),
    ("r", "rerun test"),
    ("R", "rerun failed"),
    ("f", "failures"),
    ("x", "again: skip cleanups"),
];

/// The Run tab.
#[derive(Debug)]
pub struct RunScreen {
    profiles: Vec<String>,
    model: RunModel,
    cursor: usize,
    failures_only: bool,
}

impl RunScreen {
    /// The tab before any run, running under `profiles`.
    pub fn new(profiles: Vec<String>) -> Self {
        Self { profiles, model: RunModel::default(), cursor: 0, failures_only: false }
    }

    /// The run as folded so far.
    pub const fn model(&self) -> &RunModel {
        &self.model
    }

    /// Whether only failed and blocked tests are listed.
    pub const fn failures_only(&self) -> bool {
        self.failures_only
    }

    /// The position of the chosen test among those listed.
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Whether `test` is listed: every test, or with `f` only the failed and blocked ones.
    pub const fn shows(&self, test: &Finished) -> bool {
        !self.failures_only || failing(test.status)
    }

    /// The tests listed, in file order.
    pub fn listed(&self) -> impl Iterator<Item = &Finished> {
        let tests = self.model.files.iter().flat_map(|group| group.tests.iter());
        tests.filter(|test| self.shows(test))
    }

    /// The chosen test, when any is listed.
    pub fn selected(&self) -> Option<&Finished> {
        self.listed().nth(self.cursor)
    }

    /// Starts a run of `select`, or of the last run's failures, unless one is in progress.
    fn start(&mut self, select: SelectRequest, failed: bool) -> Option<Ask> {
        if self.model.busy() {
            return None;
        }
        let plan = PlanRequest { select, profiles: self.profiles.clone(), ..PlanRequest::default() };
        self.model.ask();
        Some(Ask::Run(Box::new(RunRequest { plan, failed, ..RunRequest::default() })))
    }

    fn rerun_selected(&mut self) -> Option<Ask> {
        let test = self.selected()?.test.clone();
        self.start(SelectRequest { ids: vec![test], ..SelectRequest::default() }, false)
    }

    fn step(&mut self, down: bool) {
        self.cursor = clamp_step(self.cursor, self.listed().count(), down);
    }

    /// Stops the run: the first `x` lets its cleanups run, the next skips them, as the footer
    /// then says.
    const fn stop(&mut self) -> Ask {
        self.model.stopping = true;
        Ask::Stop(Flight::Run)
    }
}

impl Screen for RunScreen {
    fn title(&self) -> &'static str {
        "Run"
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        if self.model.stopping && self.model.busy() { &KEYS_STOPPING } else { &KEYS }
    }

    fn press(&mut self, key: KeyEvent) -> Option<Ask> {
        if let Some(down) = cursor_move(key.code) {
            self.step(down);
            return None;
        }
        match key.code {
            KeyCode::Char('a') => return self.start(SelectRequest::default(), false),
            KeyCode::Char('r') => return self.rerun_selected(),
            KeyCode::Char('R') => return self.start(SelectRequest::default(), true),
            KeyCode::Char('f') => {
                self.failures_only = !self.failures_only;
                self.cursor = 0;
            }
            KeyCode::Char('x') if self.model.busy() => return Some(self.stop()),
            _ => {}
        }
        None
    }

    fn take(&mut self, update: &Update) {
        match update {
            Update::Run(item) => {
                if matches!(item, RunFeed::Started(_)) {
                    self.cursor = 0;
                }
                self.model.feed(item);
            }
            Update::RunFailed(reason) => self.model.failed(reason),
            _ => {}
        }
    }

    fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        run_view::draw(self, frame, area);
    }
}

/// Whether a test failed or was blocked: what `f` lists and `R` reruns.
pub const fn failing(status: TestStatus) -> bool {
    matches!(status, TestStatus::Failed | TestStatus::Blocked)
}
