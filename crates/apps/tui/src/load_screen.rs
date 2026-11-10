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
use crate::screen::{Ask, Screen, Update, cursor_move};
use crate::theme::clean;

/// The Load tab's keys.
const KEYS: [(&str, &str); 4] = [("↑↓", "move"), ("enter", "run"), ("x", "stop"), ("r", "reload")];
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

    /// Runs the chosen scenario, unless a run is under way.
    fn start(&mut self) -> Option<Ask> {
        if self.busy() {
            return None;
        }
        let scenario = self.scenarios.get(self.cursor)?.id.clone();
        self.phase = LoadPhase::Starting(scenario.clone());
        self.ticks.clear();
        Some(Ask::Load(LoadRequest { scenario, profiles: self.profiles.clone(), sets: Vec::new() }))
    }

    fn step(&mut self, down: bool) {
        let last = self.scenarios.len().saturating_sub(1);
        self.cursor = if down { (self.cursor + 1).min(last) } else { self.cursor.saturating_sub(1) };
    }

    /// Keeps the closed second `event` tells; the run is under way from the first.
    fn tick(&mut self, event: &Event) {
        let (Some(tick), LoadPhase::Starting(scenario) | LoadPhase::Running(scenario)) = (Tick::of(event), &self.phase)
        else {
            return;
        };
        self.phase = LoadPhase::Running(scenario.clone());
        if self.ticks.len() == KEPT {
            self.ticks.pop_front();
        }
        self.ticks.push_back(tick);
    }

    /// Takes the load files the project lists.
    fn read(&mut self, scenarios: &[Listed]) {
        self.scenarios = scenarios.to_vec();
        self.reading = ReadState::Read;
        self.cursor = self.cursor.min(self.scenarios.len().saturating_sub(1));
    }
}

/// The first line of why something failed, safe to draw.
fn first_line(reason: &str) -> String {
    clean(reason.lines().next().unwrap_or_default())
}

impl Screen for LoadScreen {
    fn title(&self) -> &'static str {
        "Load"
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &KEYS
    }

    fn press(&mut self, key: KeyEvent) -> Option<Ask> {
        if let Some(down) = cursor_move(key.code) {
            self.step(down);
            return None;
        }
        match key.code {
            KeyCode::Enter => return self.start(),
            KeyCode::Char('x') if self.busy() => return Some(Ask::Stop),
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
            Update::Loaded(report) if self.busy() => self.phase = LoadPhase::Done(report.clone()),
            Update::LoadFailed(reason) if self.busy() => self.phase = LoadPhase::Failed(first_line(reason)),
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
