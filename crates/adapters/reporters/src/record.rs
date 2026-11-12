//! The run's record: history rows as each test finishes, and the [`RunSnapshot`] the report
//! formats render once the run ends.

use std::collections::HashMap;

use vc_interfaces::Row;
use vc_model::{ActionId, ActionOutcome, Event, InstanceId, Names, OwnerKey, TestResult, TestStatus};

use crate::collect::{Collector, TestRecord};
use crate::exchange::{self, Keeping};
use crate::view::{AbandonedView, ExchangeView, Gathered, RunFacts, TestView};
use crate::{Color, ReporterContext, RunSnapshot, encode, failure, rows, text};

/// Follows the event stream and keeps what the history and the report files need.
///
/// Exchanges are kept as they arrive, already redacted, with their bodies cut at
/// `history.maxBodySize`; `history.bodies` decides, once the test's status is known, whether its
/// bodies are kept at all.
#[derive(Debug)]
pub struct Recorder {
    project: Option<String>,
    names: Names,
    collector: Collector,
    keeping: Keeping,
    pending: HashMap<(OwnerKey, ActionId), Vec<ExchangeView>>,
    open: HashMap<InstanceId, Vec<ExchangeView>>,
    tests: Vec<TestView>,
}

impl Recorder {
    /// A recorder for the context's program and plan, keeping bodies as `history:` says.
    #[must_use]
    pub fn new(context: &ReporterContext) -> Self {
        let program = context.program();
        let history = program.config.history;
        let max_body = history.max_body();
        Self {
            project: program.config.project.clone(),
            names: Names::new(program, context.plan()),
            collector: Collector::new(context.clock()),
            keeping: Keeping { bodies: history.bodies, max_body },
            pending: HashMap::new(),
            open: HashMap::new(),
            tests: Vec::new(),
        }
    }

    /// Takes the next event and returns the history rows it completes: a finished test, or the
    /// exchanges of a lifecycle action.
    pub fn take(&mut self, event: &Event) -> Vec<Row> {
        if let Event::Exchanged { action, owner, exchange } = event {
            let view = exchange::view(exchange, self.keeping.max_body);
            let view = ExchangeView { action: self.names.action(*action), api: self.names.api_of(*action), ..view };
            self.pending.entry((*owner, *action)).or_default().push(view);
            return Vec::new();
        }
        self.collector.take(event);
        match event {
            Event::ActionFinished { action, owner, outcome } => self.settle(*action, *owner, outcome),
            Event::TestFinished { result } => self.finish_test(result),
            _ => Vec::new(),
        }
    }

    /// The snapshot of the whole run, once it has ended as `run` says.
    #[must_use]
    pub fn finish(self, run: RunFacts) -> RunSnapshot {
        let (names, collector) = (&self.names, &self.collector);
        let abandoned = collector.abandoned.iter().map(|(owner, action)| AbandonedView {
            action: names.action(*action),
            owner: encode::owner(names, *owner),
        });
        let profile_names = run.profiles.iter().filter_map(|id| Some((id.clone(), names.profile_name(id)?.to_owned())));
        let gathered = Gathered {
            project: self.project,
            profile_names: profile_names.collect(),
            files: collector.files.iter().map(|file| names.file(*file).to_owned()).collect(),
            lifecycle: collector.lifecycle.iter().map(|step| encode::step(names, step)).collect(),
            cleanups: collector.cleanups.iter().map(|cleanup| encode::cleanup(names, cleanup)).collect(),
            abandoned: abandoned.collect(),
            tests: self.tests,
        };
        RunSnapshot::new(run, gathered)
    }

    /// Files an action's exchanges under its test, or returns them as rows of the run.
    fn settle(&mut self, action: ActionId, owner: OwnerKey, outcome: &ActionOutcome) -> Vec<Row> {
        let exchanges = self.pending.remove(&(owner, action)).unwrap_or_default();
        if let OwnerKey::Instance(instance) = owner {
            self.open.entry(instance).or_default().extend(exchanges);
            return Vec::new();
        }
        let keep = self.keeping.bodies_of(outcome.failure().is_some());
        exchanges.into_iter().map(|view| Row::Exchange(rows::exchange(&kept(view, keep)))).collect()
    }

    fn finish_test(&mut self, result: &TestResult) -> Vec<Row> {
        let Some(record) = self.collector.tests.pop() else { return Vec::new() };
        let exchanges = result.instance.and_then(|instance| self.open.remove(&instance)).unwrap_or_default();
        let keep = self.keeping.bodies_of(!matches!(result.status, TestStatus::Passed | TestStatus::Skipped));
        let exchanges = exchanges.into_iter().map(|view| kept(view, keep)).collect();
        let view = test_view(&self.names, &record, exchanges);
        let row = rows::test(&self.names, &record, &view);
        self.tests.push(view);
        vec![Row::Test(row)]
    }
}

fn kept(view: ExchangeView, bodies: bool) -> ExchangeView {
    if bodies { view } else { exchange::without_bodies(view) }
}

fn test_view(names: &Names, record: &TestRecord, exchanges: Vec<ExchangeView>) -> TestView {
    let result = &record.result;
    let (example_row, repeat_index) = names.slot(result);
    let blocked = result.failure.as_ref().filter(|_| result.status == TestStatus::Blocked);
    let failed = matches!(result.status, TestStatus::Failed | TestStatus::Blocked);
    TestView {
        id: names.test(result.test).to_owned(),
        label: names.instance(result),
        name: names.test_name(result.test).map(str::to_owned),
        file: names.file_of(result.test),
        file_name: names.file_name_of(result.test).map(str::to_owned),
        status: result.status,
        instance: result.instance.map(InstanceId::index),
        example_row,
        repeat_index,
        attempts: result.attempts,
        start_ms: record.start.map(text::millis),
        stop_ms: record.start.map_or(0, |start| text::millis(start.saturating_add(record.duration()))),
        location: names.test_location(result.test),
        tags: names.tags(result.test),
        failure: result.failure.as_ref().map(|failed| encode::failure(names, failed)),
        blocked_by: blocked.and_then(|failed| failed.action).map(|action| names.action(action)),
        skip_reason: result.skip_reason.clone(),
        message: message(result),
        detail: if failed { failure::block(names, record, Color::Plain) } else { String::new() },
        steps: record.steps.iter().map(|step| encode::step(names, step)).collect(),
        exchanges,
    }
}

/// The failure that decided the status, or the skip reason.
fn message(result: &TestResult) -> Option<String> {
    match result.status {
        TestStatus::Skipped => result.skip_reason.clone(),
        TestStatus::Passed => None,
        TestStatus::Failed | TestStatus::Blocked | TestStatus::Flaky | TestStatus::AllowedFailure => {
            result.failure.as_ref().map(|failure| failure.message.clone())
        }
    }
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
