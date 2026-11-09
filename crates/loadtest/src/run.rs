//! A load run: the stage loop, the virtual users and the recorder, second by second.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::time::Instant;
use vc_executor::{Arguments, CancelTree, Iterations, Judge, MatcherRegistry};
use vc_interfaces::Random;
use vc_model::LoadScenario;

use crate::user::{Crew, Seat, user};
use crate::{
    Breach, Interval, LoadError, Mix, Recorder, Sample, StageCommand, StageController, StageInput, Totals, Window,
    aborts, breaches,
};

/// What judges a load run's thresholds: the matchers, and what renders their arguments. A load
/// run holds it across awaits, so the renderer is `Sync`; each judgement builds its [`Judge`].
pub type Matching<'a> = (&'a MatcherRegistry, &'a (dyn Arguments + Sync));

/// What a load run needs besides its scenario and the matching of its thresholds.
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the engine and by load-engine tests; every user is in this repo (D160)"
)]
pub struct LoadParts {
    /// Runs one instance of a mix test, guarding its actions with `cancel`.
    pub iterations: Arc<dyn Iterations>,
    /// Picks each iteration's test and think time.
    pub random: Arc<dyn Random>,
    /// The run's frame: cancelling it (Ctrl-C) ends the run; an `abortOn` breach cancels it.
    pub cancel: CancelTree,
    /// Receives each second as it closes; a receiver that has gone away is not an error.
    pub seconds: mpsc::Sender<Second>,
}

/// One closed second of a load run, for the live view and the store.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Second {
    /// Seconds since the run started: every second before this one is closed.
    pub at: u64,
    /// The users that ran during the second just closed.
    pub users: u32,
    /// The rows of the seconds closed now, by second and then test.
    pub rows: Vec<Interval>,
    /// The rolling window's totals, over the last [`crate::WINDOW_SECONDS`].
    pub window: Totals,
    /// The thresholds failing in the rolling window.
    pub breaching: Vec<Breach>,
}

/// How a load run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ending {
    /// The last stage ended.
    Finished,
    /// A rolling breach of an `abortOn` metric cancelled the run.
    Aborted,
    /// The run's frame was cancelled from outside (Ctrl-C).
    Cancelled,
}

/// What a load run did, with the final verdict on its thresholds.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LoadOutcome {
    /// How it ended.
    pub ending: Ending,
    /// The whole run's iterations.
    pub totals: Totals,
    /// The thresholds the whole run breaches: exit code 3 comes from these alone.
    pub breaches: Vec<Breach>,
}

/// Runs `scenario`'s stages with `parts`, judging its thresholds with `matching`.
///
/// Once a second the stage loop closes the second before, sends it to `parts.seconds`, judges
/// the thresholds on the rolling window and adjusts the users toward the stage's target. When
/// the last stage ends (or the run is aborted or cancelled) every user retires after its
/// iteration in flight, and the whole run is judged.
///
/// # Errors
///
/// [`LoadError`] for an empty mix, a threshold that cannot be judged or a histogram that
/// cannot be kept; the run's frame is cancelled first, so no user keeps running.
pub async fn run_load(
    scenario: &LoadScenario,
    parts: LoadParts,
    matching: Matching<'_>,
) -> Result<LoadOutcome, LoadError> {
    let cancel = parts.cancel.clone();
    let outcome = drive(scenario, parts, matching).await;
    if outcome.is_err() {
        cancel.cancel();
    }
    outcome
}

async fn drive(scenario: &LoadScenario, parts: LoadParts, matching: Matching<'_>) -> Result<LoadOutcome, LoadError> {
    let (samples, mut inbox) = mpsc::unbounded_channel();
    let (min, max) = scenario.think_time;
    let crew = Crew {
        iterations: parts.iterations,
        random: parts.random,
        mix: Mix::new(&scenario.mix)?,
        think: (min.get(), max.get()),
        start: Instant::now(),
        samples,
    };
    let mut run = Run::new(scenario, Arc::new(crew), parts.cancel)?;
    let mut ticks = tokio::time::interval(Duration::from_secs(1));
    while !run.controller.finished() {
        let cancel = run.cancel.clone();
        tokio::select! {
            biased;
            () = cancel.cancelled(), if run.ending == Ending::Finished => run.stop(Ending::Cancelled),
            _ = ticks.tick() => {
                let second = run.tick(&judge(matching))?;
                send(&parts.seconds, second).await;
            }
            Some(sample) = inbox.recv() => run.recorder.record(sample)?,
        }
    }
    run.finish(&mut inbox, (&parts.seconds, matching)).await
}

fn judge((registry, args): Matching<'_>) -> Judge<'_> {
    Judge::new(registry, args)
}

async fn send(seconds: &mpsc::Sender<Second>, second: Option<Second>) {
    if let Some(second) = second {
        seconds.send(second).await.ok();
    }
}

/// The state of one run, owned by its stage loop.
struct Run<'s> {
    scenario: &'s LoadScenario,
    crew: Option<Arc<Crew>>,
    cancel: CancelTree,
    controller: StageController,
    ending: Ending,
    ticks: u64,
    seats: Vec<CancelTree>,
    recorder: Recorder,
    window: Window,
    totals: Totals,
}

impl<'s> Run<'s> {
    fn new(scenario: &'s LoadScenario, crew: Arc<Crew>, cancel: CancelTree) -> Result<Self, LoadError> {
        Ok(Self {
            scenario,
            crew: Some(crew),
            cancel,
            controller: StageController::new(scenario.stages.clone()),
            ending: Ending::Finished,
            ticks: 0,
            seats: Vec::new(),
            recorder: Recorder::new(),
            window: Window::new(),
            totals: Totals::new()?,
        })
    }

    /// A second boundary: closes the second before, judges the rolling window and moves the
    /// stages on. The first tick, at the run's start, closes nothing.
    fn tick(&mut self, judge: &Judge<'_>) -> Result<Option<Second>, LoadError> {
        let at = self.ticks;
        self.ticks = self.ticks.saturating_add(1);
        let second = if at == 0 { None } else { Some(self.second(at, judge)?) };
        if second.as_ref().is_some_and(|second| aborts(self.scenario, &second.breaching)) {
            self.cancel.cancel();
            self.stop(Ending::Aborted);
        } else {
            self.step(StageInput::Tick);
        }
        Ok(second)
    }

    /// Closes every second before `at` and judges the rolling window that ends there.
    fn second(&mut self, at: u64, judge: &Judge<'_>) -> Result<Second, LoadError> {
        let rows = self.recorder.close(at);
        self.total(&rows)?;
        self.window.advance(at, rows.iter().cloned());
        let window = self.window.totals()?;
        let breaching = breaches(self.scenario, &window, judge)?;
        Ok(Second { at, users: self.controller.users(), rows, window, breaching })
    }

    fn total(&mut self, rows: &[Interval]) -> Result<(), LoadError> {
        rows.iter().try_for_each(|row| self.totals.add(row))
    }

    fn stop(&mut self, ending: Ending) {
        self.ending = ending;
        self.step(StageInput::Abort);
    }

    fn step(&mut self, input: StageInput) {
        let (controller, commands) = self.controller.clone().transition(input);
        self.controller = controller;
        for command in commands {
            match command {
                StageCommand::Spawn(users) => self.spawn(users),
                StageCommand::Retire(users) => self.retire(users),
                StageCommand::Finish => self.crew = None,
            }
        }
    }

    fn spawn(&mut self, users: u32) {
        let Some(crew) = &self.crew else { return };
        for _ in 0..users {
            let retire = self.cancel.child();
            let number = u32::try_from(self.seats.len()).unwrap_or(u32::MAX).saturating_add(1);
            let seat = Seat { user: number, run: self.cancel.clone(), retire: retire.clone() };
            self.seats.push(retire);
            tokio::spawn(user(Arc::clone(crew), seat));
        }
    }

    fn retire(&mut self, users: u32) {
        for _ in 0..users {
            if let Some(seat) = self.seats.pop() {
                seat.cancel();
            }
        }
    }

    /// Waits for the users still in an iteration, sends the seconds still open as one last
    /// [`Second`] (no users, the window as it was, nothing judged) and judges the whole run over
    /// the seconds up to the last tick.
    async fn finish(
        mut self,
        inbox: &mut UnboundedReceiver<Sample>,
        (seconds, matching): (&mpsc::Sender<Second>, Matching<'_>),
    ) -> Result<LoadOutcome, LoadError> {
        let span = self.ticks.saturating_sub(1);
        while let Some(sample) = inbox.recv().await {
            self.recorder.record(sample)?;
        }
        let rows = self.recorder.close(u64::MAX);
        self.total(&rows)?;
        let window = self.window.totals()?;
        send(seconds, Some(Second { at: span, users: 0, rows, window, breaching: Vec::new() })).await;
        self.totals.cover(span);
        let breaches = breaches(self.scenario, &self.totals, &judge(matching))?;
        Ok(LoadOutcome { ending: self.ending, totals: self.totals, breaches })
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
