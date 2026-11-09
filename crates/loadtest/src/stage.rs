//! The stage controller: how many virtual users run at each second of a load run.

use vc_model::LoadStage;

/// What the load shell feeds a [`StageController`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StageInput {
    /// A second boundary: the first tick is the run's start, each later one a second on.
    Tick,
    /// An `abortOn` threshold was breached or the run was cancelled.
    Abort,
}

/// What a [`StageController`] asks the load shell to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StageCommand {
    /// Start this many more user tasks.
    Spawn(u32),
    /// Stop this many user tasks once their current iteration ends.
    Retire(u32),
    /// The last stage is over: no user runs any more.
    Finish,
}

/// Adjusts the number of virtual users once per second toward the stage target.
///
/// A pure state machine (the LLD's "load stage controller"): users ramp linearly from the
/// previous stage's count, starting at 0, to the stage's `users` over its `duration`, rounded
/// to the nearest user. At the tick on or after the last stage's end, every user retires and
/// the run finishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageController {
    stages: Vec<LoadStage>,
    ticks: u64,
    users: u32,
    finished: bool,
}

impl StageController {
    /// A controller over `stages`, before its first tick, with no users.
    #[must_use]
    pub const fn new(stages: Vec<LoadStage>) -> Self {
        Self { stages, ticks: 0, users: 0, finished: false }
    }

    /// The users running after the commands so far.
    #[must_use]
    pub const fn users(&self) -> u32 {
        self.users
    }

    /// Whether the controller has finished.
    #[must_use]
    pub const fn finished(&self) -> bool {
        self.finished
    }

    /// The next state and the commands `input` calls for; a finished controller ignores input.
    #[must_use]
    pub fn transition(mut self, input: StageInput) -> (Self, Vec<StageCommand>) {
        if self.finished {
            return (self, Vec::new());
        }
        let target = match input {
            StageInput::Tick => target(&self.stages, u128::from(self.ticks) * 1000),
            StageInput::Abort => None,
        };
        self.ticks = self.ticks.saturating_add(1);
        let commands = match target {
            Some(users) => self.adjust(users).into_iter().collect(),
            None => self.finish(),
        };
        (self, commands)
    }

    fn adjust(&mut self, target: u32) -> Option<StageCommand> {
        let current = self.users;
        self.users = target;
        match target.cmp(&current) {
            std::cmp::Ordering::Greater => Some(StageCommand::Spawn(target - current)),
            std::cmp::Ordering::Less => Some(StageCommand::Retire(current - target)),
            std::cmp::Ordering::Equal => None,
        }
    }

    fn finish(&mut self) -> Vec<StageCommand> {
        self.finished = true;
        self.adjust(0).into_iter().chain([StageCommand::Finish]).collect()
    }
}

/// The users `stages` call for `at_ms` into the run; `None` once the last stage is over.
fn target(stages: &[LoadStage], at_ms: u128) -> Option<u32> {
    let mut start = 0_u128;
    let mut previous = 0_u128;
    for stage in stages {
        let length = stage.duration.get().as_millis();
        let to = u128::from(stage.users);
        if at_ms < start.saturating_add(length) {
            let into = at_ms - start;
            let users = (previous * (length - into) + to * into + length / 2) / length;
            return Some(u32::try_from(users).unwrap_or(u32::MAX));
        }
        start = start.saturating_add(length);
        previous = to;
    }
    None
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;
