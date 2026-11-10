//! The Load tab: the project's load scenarios, and the chosen one run with `enter` and shown
//! live as `vc load` prints it (G13): one line a second with the users, requests per second,
//! latencies, errors and the thresholds breaching, then the run's summary.

use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use vc_engine::{Event, Listed, LoadReport, LoadRequest};

use crate::history_screen::ReadState;
use crate::load_view;
use crate::screen::{Ask, Flight, Screen, Update, clamp_step, cursor_move};
use crate::theme::{clean, first_line};

/// The Load tab's keys.
const KEYS: [(&str, &str); 4] = [("↑↓", "move"), ("enter", "run"), ("x", "stop"), ("r", "reload")];
/// The Load tab's keys once `x` asked the load run to stop.
const KEYS_STOPPING: [(&str, &str); 4] =
    [("↑↓", "move"), ("enter", "run"), ("x", "again: skip cleanups"), ("r", "reload")];
/// The seconds the tab keeps, newest last.
const KEPT: usize = 120;

/// Where the load run has got to.
#[derive(Clone, Debug, Default)]
pub enum LoadPhase {
    /// No run yet.
    #[default]
    Idle,
    /// Asked for this scenario; no second has closed yet.
    Starting(String),
    /// This scenario is running.
    Running(String),
    /// The run is over.
    Done(Box<LoadReport>),
    /// The run could not start or ended in an error: the first line of why.
    Failed(String),
}

/// One closed second, as [`Event::LoadTick`] tells it, latencies in whole ms.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    /// The second of the run, from 1.
    pub second: u64,
    /// The virtual users running.
    pub users: u32,
    /// Iterations per second.
    pub rps: u64,
    /// The 50th, 95th and 99th percentile latencies.
    pub latencies: [u128; 3],
    /// Failed iterations.
    pub errors: u64,
    /// The thresholds breaching.
    pub breaching: Vec<String>,
}

impl Tick {
    /// The tick `event` tells, if it is one.
    fn of(event: &Event) -> Option<Self> {
        let Event::LoadTick { second, users, rps, p50, p95, p99, errors, breaching } = event else {
            return None;
        };
        let latencies = [p50.as_millis(), p95.as_millis(), p99.as_millis()];
        let breaching = breaching.iter().map(|name| clean(name)).collect();
        Some(Self { second: *second, users: *users, rps: *rps, latencies, errors: *errors, breaching })
    }
}

/// The Load tab.
#[derive(Debug, Default)]
pub struct LoadScreen {
    profiles: Vec<String>,
    reading: ReadState,
    scenarios: Vec<Listed>,
    cursor: usize,
    phase: LoadPhase,
    ticks: VecDeque<Tick>,
    stopping: bool,
    refused: Option<String>,
    before: Option<LoadPhase>,
}

impl LoadScreen {
    /// The tab before the scenarios are read, running under `profiles`.
    pub fn new(profiles: Vec<String>) -> Self {
        Self { profiles, ..Self::default() }
    }

    /// Where reading the scenarios has got to.
    pub const fn reading(&self) -> &ReadState {
        &self.reading
    }

    /// The load scenarios, in project order.
    pub fn scenarios(&self) -> &[Listed] {
        &self.scenarios
    }

    /// The chosen scenario's position.
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Where the load run has got to.
    pub const fn phase(&self) -> &LoadPhase {
        &self.phase
    }

    /// The closed seconds kept, oldest first.
    pub const fn ticks(&self) -> &VecDeque<Tick> {
        &self.ticks
    }

    /// Whether `x` asked the load run to stop.
    pub const fn stopping(&self) -> bool {
        self.stopping
    }

    /// Why the load run asked for did not start, the last one's still shown.
    pub fn refused(&self) -> Option<&str> {
        self.refused.as_deref()
    }

    /// Whether a run is starting or running.
    const fn busy(&self) -> bool {
        matches!(self.phase, LoadPhase::Starting(_) | LoadPhase::Running(_))
    }

    /// Asks for the scenarios again, unless they are being read.
    fn reload(&mut self) -> Option<Ask> {
        if self.reading == ReadState::Reading {
            return None;
        }
        self.reading = ReadState::Reading;
        Some(Ask::Scenarios)
    }

    /// Runs the chosen scenario, unless a run is under way. The last run's seconds or summary
    /// stay until the new one's first second, so a run that is refused or cannot start leaves
    /// them shown.
    fn start(&mut self) -> Option<Ask> {
        if self.busy() {
            return None;
        }
        let scenario = self.scenarios.get(self.cursor)?.id.clone();
        self.before = Some(std::mem::replace(&mut self.phase, LoadPhase::Starting(scenario.clone())));
        (self.stopping, self.refused) = (false, None);
        Some(Ask::Load(LoadRequest { scenario, profiles: self.profiles.clone(), sets: Vec::new() }))
    }

    fn step(&mut self, down: bool) {
        self.cursor = clamp_step(self.cursor, self.scenarios.len(), down);
    }

    /// Keeps the closed second `event` tells; the run is under way from the first.
    fn tick(&mut self, event: &Event) {
        let (Some(tick), LoadPhase::Starting(scenario) | LoadPhase::Running(scenario)) = (Tick::of(event), &self.phase)
        else {
            return;
        };
        let scenario = scenario.clone();
        self.begun();
        self.phase = LoadPhase::Running(scenario);
        if self.ticks.len() == KEPT {
            self.ticks.pop_front();
        }
        self.ticks.push_back(tick);
    }

    /// The run asked for has begun: the last one's seconds go.
    fn begun(&mut self) {
        if matches!(self.phase, LoadPhase::Starting(_)) {
            self.ticks.clear();
            self.before = None;
        }
    }

    /// The load run is over.
    fn loaded(&mut self, report: &LoadReport) {
        self.begun();
        self.phase = LoadPhase::Done(Box::new(report.clone()));
    }

    /// The load run could not start, the last one kept, or it ended in an error.
    fn failed(&mut self, reason: &str) {
        let reason = first_line(reason);
        match self.before.take() {
            Some(before) if matches!(self.phase, LoadPhase::Starting(_)) => {
                (self.phase, self.refused) = (before, Some(reason));
            }
            _ => self.phase = LoadPhase::Failed(reason),
        }
    }

    /// Stops the load run: the first `x` lets its cleanups run, the next skips them, as the
    /// footer then says.
    const fn stop(&mut self) -> Ask {
        self.stopping = true;
        Ask::Stop(Flight::Load)
    }

    /// Takes the load files the project lists.
    fn read(&mut self, scenarios: &[Listed]) {
        self.scenarios = scenarios.to_vec();
        self.reading = ReadState::Read;
        self.cursor = self.cursor.min(self.scenarios.len().saturating_sub(1));
    }
}

impl Screen for LoadScreen {
    fn title(&self) -> &'static str {
        "Load"
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        if self.stopping && self.busy() { &KEYS_STOPPING } else { &KEYS }
    }

    fn press(&mut self, key: KeyEvent) -> Option<Ask> {
        if let Some(down) = cursor_move(key.code) {
            self.step(down);
            return None;
        }
        match key.code {
            KeyCode::Enter => return self.start(),
            KeyCode::Char('x') if self.busy() => return Some(self.stop()),
            KeyCode::Char('r') => return self.reload(),
            _ => {}
        }
        None
    }

    fn take(&mut self, update: &Update) {
        match update {
            Update::Scenarios(scenarios) => self.read(scenarios),
            Update::ScenariosFailed(reason) => self.reading = ReadState::Failed(first_line(reason)),
            Update::Load(event) => self.tick(event),
            Update::Loaded(report) if self.busy() => self.loaded(report),
            Update::LoadFailed(reason) if self.busy() => self.failed(reason),
            _ => {}
        }
    }

    fn shown(&mut self) -> Option<Ask> {
        if self.reading == ReadState::Unread { self.reload() } else { None }
    }

    fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        load_view::draw(self, frame, area);
    }
}
