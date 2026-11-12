//! Every cleanup a file or the run registers is finished or abandoned exactly once, however an
//! abort meets the cleanups: before the worker's reply or after it, stopping the cleanup or not.

use std::collections::VecDeque;
use std::sync::Arc;

use proptest::prelude::{Strategy, prop, prop_assert, prop_assert_eq, proptest};
use vc_model::{Event, Plan, TestStatus};

use super::sim;
use super::{ActionReport, FileCommand, FileEnd, FileInput, FileOrchestrator, RunCommand, RunInput, RunOrchestrator};

/// An abort landing on the `at`th cleanup: before the worker replies, or after when
/// `reply_first`. The worker replies that the abort stopped it when `stopped`, and so do the
/// cleanups still in flight after it.
#[derive(Clone, Copy, Debug)]
struct Abort {
    at: usize,
    stopped: bool,
    reply_first: bool,
}

fn abort() -> impl Strategy<Value = Option<Abort>> {
    let abort = (0usize..6, prop::arbitrary::any::<bool>(), prop::arbitrary::any::<bool>());
    prop::option::of(abort.prop_map(|(at, stopped, reply_first)| Abort { at, stopped, reply_first }))
}

/// What a command asks of the driver.
enum Asks<I> {
    Action,
    Cleanup(u32),
    Abandoned(Vec<u32>),
    Inputs(Vec<I>),
    Nothing,
}

/// A frame machine as the driver sees it.
trait Frame: Sized {
    type Input;
    type Command;
    fn feed(self, input: Self::Input) -> (Self, Vec<Self::Command>);
    fn abort() -> Vec<Self::Input>;
    fn acted(report: ActionReport) -> Self::Input;
    fn cleaned(stopped: bool) -> Self::Input;
    fn asks(command: &Self::Command) -> Asks<Self::Input>;
}

/// Drives a frame, its `n`th action registering `counts[n]` cleanups.
struct Driver<F: Frame> {
    frame: Option<F>,
    pending: VecDeque<F::Command>,
    counts: VecDeque<u32>,
    registered: Vec<u32>,
    finished: Vec<u32>,
    abandoned: Vec<Vec<u32>>,
    cleanups: usize,
    stopping: bool,
}

impl<F: Frame> Driver<F> {
    fn drive(frame: F, (start, counts): (F::Input, &[u32]), abort: Option<Abort>) -> Self {
        let mut driver = Self {
            frame: Some(frame),
            pending: VecDeque::new(),
            counts: counts.iter().copied().collect(),
            registered: Vec::new(),
            finished: Vec::new(),
            abandoned: Vec::new(),
            cleanups: 0,
            stopping: false,
        };
        driver.feed([start]);
        while let Some(command) = driver.pending.pop_front() {
            match F::asks(&command) {
                Asks::Action => driver.act(),
                Asks::Cleanup(id) => driver.clean(id, abort),
                Asks::Abandoned(ids) => driver.abandoned.push(ids),
                Asks::Inputs(inputs) => driver.feed(inputs),
                Asks::Nothing => {}
            }
        }
        driver
    }

    fn feed(&mut self, inputs: impl IntoIterator<Item = F::Input>) {
        for input in inputs {
            let Some(frame) = self.frame.take() else { return };
            let (frame, commands) = frame.feed(input);
            self.frame = Some(frame);
            self.pending.extend(commands);
        }
    }

    fn act(&mut self) {
        let first = 1000 + u32::try_from(self.registered.len()).unwrap_or(0);
        let ids: Vec<u32> = (first..first + self.counts.pop_front().unwrap_or(0)).collect();
        self.registered.extend(&ids);
        self.feed([F::acted(sim::passing_with(&ids))]);
    }

    fn clean(&mut self, id: u32, abort: Option<Abort>) {
        let here = abort.filter(|abort| abort.at == self.cleanups);
        self.cleanups += 1;
        let Some(abort) = here else { return self.reply(id, self.stopping) };
        if abort.reply_first {
            self.reply(id, abort.stopped);
        }
        self.stopping = abort.stopped;
        self.feed(F::abort());
        if !abort.reply_first {
            self.reply(id, abort.stopped);
        }
    }

    fn reply(&mut self, id: u32, stopped: bool) {
        if !stopped {
            self.finished.push(id);
        }
        self.feed([F::cleaned(stopped)]);
    }
}

impl Frame for FileOrchestrator {
    type Input = FileInput;
    type Command = FileCommand;

    fn feed(self, input: FileInput) -> (Self, Vec<FileCommand>) {
        self.transition(input)
    }

    fn abort() -> Vec<FileInput> {
        vec![FileInput::Abort]
    }

    fn acted(report: ActionReport) -> FileInput {
        FileInput::ActionDone(Box::new(report))
    }

    fn cleaned(stopped: bool) -> FileInput {
        if stopped { FileInput::CleanupAborted } else { FileInput::CleanupDone }
    }

    fn asks(command: &FileCommand) -> Asks<FileInput> {
        match command {
            FileCommand::RunAction(_) => Asks::Action,
            FileCommand::RunCleanup(cleanup) => Asks::Cleanup(cleanup.action.index()),
            FileCommand::Launch(_) => Asks::Inputs(vec![FileInput::InstanceDone(TestStatus::Passed)]),
            FileCommand::Emit(Event::CleanupsAbandoned { cleanups, .. }) => {
                Asks::Abandoned(cleanups.iter().map(|action| action.index()).collect())
            }
            _ => Asks::Nothing,
        }
    }
}

impl Frame for RunOrchestrator {
    type Input = RunInput;
    type Command = RunCommand;

    fn feed(self, input: RunInput) -> (Self, Vec<RunCommand>) {
        self.transition(input)
    }

    fn abort() -> Vec<RunInput> {
        vec![RunInput::Interrupt, RunInput::Interrupt, RunInput::Interrupt]
    }

    fn acted(report: ActionReport) -> RunInput {
        RunInput::ActionDone(report)
    }

    fn cleaned(stopped: bool) -> RunInput {
        if stopped { RunInput::CleanupAborted } else { RunInput::CleanupDone }
    }

    fn asks(command: &RunCommand) -> Asks<RunInput> {
        match command {
            RunCommand::RunAction(_) => Asks::Action,
            RunCommand::RunCleanup(cleanup) => Asks::Cleanup(cleanup.action.index()),
            RunCommand::StartFiles(_) => Asks::Inputs(vec![RunInput::FileDone(FileEnd::default())]),
            RunCommand::Report(_) => Asks::Inputs(vec![RunInput::Reported]),
            RunCommand::Emit(Event::CleanupsAbandoned { cleanups, .. }) => {
                Asks::Abandoned(cleanups.iter().map(|action| action.index()).collect())
            }
            _ => Asks::Nothing,
        }
    }
}

/// Checks that what `driver` saw closes each registered cleanup once: finished ones newest
/// first, then one abandoned list, led by the cleanup an abort stopped in flight.
fn closes_each_once<F: Frame>(driver: &Driver<F>) -> Result<(), proptest::test_runner::TestCaseError> {
    prop_assert!(driver.abandoned.len() <= 1, "one event per frame: {:?}", driver.abandoned);
    let closed: Vec<u32> = driver.finished.iter().chain(driver.abandoned.iter().flatten()).copied().collect();
    let newest_first: Vec<u32> = driver.registered.iter().rev().copied().collect();
    prop_assert_eq!(closed, newest_first);
    Ok(())
}

proptest! {
    #[test]
    fn a_file_finishes_or_abandons_each_cleanup_once(counts in prop::collection::vec(0u32..3, 0..4), abort in abort()) {
        let before_all = (1..=u32::try_from(counts.len()).unwrap_or(0)).map(|id| sim::planned(id, &[])).collect();
        let file = FileOrchestrator::new(sim::planned_file(before_all, vec![sim::instance(0, &[])], &[]), 1);
        let driver = Driver::drive(file, (FileInput::Start(super::FailedActions::new()), &counts), abort);
        closes_each_once(&driver)?;
    }

    #[test]
    fn the_run_finishes_or_abandons_each_cleanup_once(counts in prop::collection::vec(0u32..3, 0..4), abort in abort()) {
        let before_run = (1..=u32::try_from(counts.len()).unwrap_or(0)).map(|id| sim::planned(id, &[])).collect();
        let files = vec![(*sim::planned_file(vec![], vec![], &[])).clone()];
        let run = RunOrchestrator::new(Arc::new(Plan { before_run, files, ..Plan::default() }), false);
        let driver = Driver::drive(run, (RunInput::Start, &counts), abort);
        closes_each_once(&driver)?;
    }
}
