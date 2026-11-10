use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use vc_model::{ActionId, ActionOutcome, CheckResult, CleanupRecord, Event, FileId, InstanceId, OwnerKey, TestResult};

use crate::Stopwatch;

/// One action that ran, with the checks it evaluated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepRecord {
    /// The action.
    pub action: ActionId,
    /// The frame it ran in.
    pub owner: OwnerKey,
    /// How it ended.
    pub outcome: ActionOutcome,
    /// Its checks, in the order evaluated.
    pub checks: Vec<CheckResult>,
}

/// One finished test instance, or a skipped test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestRecord {
    /// The result.
    pub result: TestResult,
    /// The actions the instance ran (retries and loops included), in order.
    pub steps: Vec<StepRecord>,
    /// When its first action started, from the start of the run.
    pub start: Option<Duration>,
    /// When it finished, from the start of the run.
    pub stop: Duration,
}

impl TestRecord {
    /// How long the instance ran; zero when it never started.
    pub fn duration(&self) -> Duration {
        self.start.map_or(Duration::ZERO, |start| self.stop.saturating_sub(start))
    }

    /// The checks that did not hold, in order.
    pub fn failed_checks(&self) -> impl Iterator<Item = &CheckResult> {
        self.steps.iter().flat_map(|step| &step.checks).filter(|check| !check.passed)
    }
}

/// Everything a file report needs, gathered from the event stream.
#[derive(Debug)]
pub struct Collector {
    stopwatch: Arc<dyn Stopwatch>,
    /// Checks evaluated by actions that have not finished yet.
    pending: HashMap<(OwnerKey, ActionId), Vec<CheckResult>>,
    /// Steps and start time of instances that have not finished yet.
    open: HashMap<InstanceId, (Duration, Vec<StepRecord>)>,
    /// The files that started, in order.
    pub files: Vec<FileId>,
    /// Finished instances and skipped tests, in the order they finished.
    pub tests: Vec<TestRecord>,
    /// `beforeRun`, `afterRun`, `beforeAll` and `afterAll` actions, in the order they finished.
    pub lifecycle: Vec<StepRecord>,
    /// Cleanups that ran.
    pub cleanups: Vec<CleanupRecord>,
    /// Cleanups a third Ctrl-C abandoned, with the frame that registered them.
    pub abandoned: Vec<(OwnerKey, ActionId)>,
}

impl Collector {
    /// An empty collector timing instances with `stopwatch`.
    pub fn new(stopwatch: Arc<dyn Stopwatch>) -> Self {
        Self {
            stopwatch,
            pending: HashMap::new(),
            open: HashMap::new(),
            files: Vec::new(),
            tests: Vec::new(),
            lifecycle: Vec::new(),
            cleanups: Vec::new(),
            abandoned: Vec::new(),
        }
    }

    /// Takes the next event.
    pub fn take(&mut self, event: &Event) {
        match event {
            Event::FileStarted { file } => self.files.push(*file),
            Event::ActionStarted { owner: OwnerKey::Instance(instance), .. } => self.start(*instance),
            Event::CheckEvaluated { action, owner, check } => {
                self.pending.entry((*owner, *action)).or_default().push(check.as_ref().clone());
            }
            Event::ActionFinished { action, owner, outcome } => self.finish_action(*action, *owner, outcome),
            Event::CleanupFinished { cleanup } => self.cleanups.push(cleanup.clone()),
            Event::CleanupsAbandoned { owner, cleanups } => {
                self.abandoned.extend(cleanups.iter().map(|action| (*owner, *action)));
            }
            Event::TestFinished { result } => self.finish_test(result),
            _ => {}
        }
    }

    fn start(&mut self, instance: InstanceId) {
        let now = self.stopwatch.elapsed();
        self.open.entry(instance).or_insert_with(|| (now, Vec::new()));
    }

    fn finish_action(&mut self, action: ActionId, owner: OwnerKey, outcome: &ActionOutcome) {
        let checks = self.pending.remove(&(owner, action)).unwrap_or_default();
        let step = StepRecord { action, owner, outcome: outcome.clone(), checks };
        match owner {
            OwnerKey::Instance(instance) => self.open.entry(instance).or_default().1.push(step),
            OwnerKey::Run | OwnerKey::File(_) => self.lifecycle.push(step),
        }
    }

    fn finish_test(&mut self, result: &TestResult) {
        let stop = self.stopwatch.elapsed();
        let open = result.instance.and_then(|instance| self.open.remove(&instance));
        let (start, steps) = open.map_or((None, Vec::new()), |(start, steps)| (Some(start), steps));
        self.tests.push(TestRecord { result: result.clone(), steps, start, stop });
    }
}

#[cfg(test)]
#[path = "collect_tests.rs"]
mod tests;
