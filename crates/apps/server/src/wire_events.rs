//! The run feed in its wire form (C6c): each event of a run as the `RunEvent` the web UI's run
//! view reads, labelled by the run's [`Names`]. The engine masks every event with the run's
//! redactor before the feed carries it; nothing here adds a value the run did not send.
//!
//! Sequence numbers: an event keeps the run's own; `RunPlanned` shares `RunStarted`'s, since the
//! plan is part of the start; `Missed` carries the last number seen; the report carries the
//! number after it.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vc_contracts::engine::v1 as wire;
use vc_contracts::engine::v1::owner;
use vc_contracts::engine::v1::run_event::Kind;
use vc_contracts::results::v1 as results;
use vc_engine::{
    ActionId, CleanupRecord, Event, Failure, Names, OwnerKey, Planned, RunFeed, RunReport, RunStatus, Sequenced,
    Totals, action_call,
};

use crate::wire_history::{run_status, totals};
use crate::wire_results;

/// The run feed in wire form: the names that label its events, and the last number seen.
#[derive(Default)]
pub struct WireFeed {
    /// The run's names, once the feed has started.
    names: Option<Names>,
    /// The sequence number of the last event seen.
    seq: u64,
}

impl WireFeed {
    /// The events to send for `item`, in order; none for the names that start the feed.
    pub fn events(&mut self, item: RunFeed) -> Vec<wire::RunEvent> {
        match item {
            RunFeed::Started(names) => {
                self.names = Some(names);
                Vec::new()
            }
            RunFeed::Event(sequenced) => self.event(sequenced),
            RunFeed::Missed { count } => vec![at(self.seq, Kind::Missed(wire::Missed { count }))],
            RunFeed::Done(report) => vec![at(self.seq.saturating_add(1), Kind::Report(run_report(report)))],
            _ => Vec::new(),
        }
    }

    /// `sequenced` in wire form; `RunStarted` is followed by `RunPlanned`.
    fn event(&mut self, sequenced: Sequenced) -> Vec<wire::RunEvent> {
        let Sequenced { seq, event } = sequenced;
        self.seq = seq;
        let Some(names) = &self.names else { return Vec::new() };
        let started = matches!(event, Event::RunStarted { .. });
        let mut events = vec![at(seq, kind(names, event))];
        if started {
            events.push(at(seq, Kind::RunPlanned(run_planned(names))));
        }
        events
    }
}

/// An action's reference and its owner, as every action event carries them.
struct Scope {
    /// The action.
    action: Option<wire::ActionRef>,
    /// What it ran for.
    owner: Option<wire::Owner>,
}

/// One event of the run in wire form.
fn kind(names: &Names, event: Event) -> Kind {
    match event {
        Event::RunStarted { files, instances } => Kind::RunStarted(run_started(files, instances)),
        Event::FileStarted { file } => Kind::FileStarted(wire::FileStarted { file: names.file(file).to_owned() }),
        Event::FileFinished { file, totals: sum } => {
            Kind::FileFinished(wire::FileFinished { file: names.file(file).to_owned(), totals: Some(totals(&sum)) })
        }
        Event::ActionStarted { action, owner } => Kind::ActionStarted(started(scope(names, action, owner))),
        Event::ActionFinished { action, owner, outcome } => {
            Kind::ActionFinished(finished(names, scope(names, action, owner), outcome.failure()))
        }
        Event::Exchanged { action, owner, exchange } => {
            Kind::Exchanged(exchanged(scope(names, action, owner), wire_results::exchange(names, action, &exchange)))
        }
        Event::CheckEvaluated { action, owner, check } => {
            Kind::CheckEvaluated(evaluated(scope(names, action, owner), wire_results::check(names, &check)))
        }
        Event::Warned { action, owner, warning } => {
            Kind::Warned(warned(scope(names, action, owner), wire_results::failure(names, &warning)))
        }
        Event::CleanupFinished { cleanup } => Kind::CleanupFinished(cleanup_finished(names, &cleanup)),
        Event::CleanupsAbandoned { owner, cleanups } => Kind::CleanupsAbandoned(abandoned(names, owner, &cleanups)),
        Event::TestFinished { result } => {
            Kind::TestFinished(wire::TestFinished { test: Some(wire_results::test(names, &result)) })
        }
        Event::RunFinished { status, totals: sum } => Kind::RunFinished(run_finished(status, &sum)),
        Event::LoadTick { second, users, rps, p50, p95, p99, errors, breaching } => {
            let (p50_ms, p95_ms, p99_ms) = (millis(p50), millis(p95), millis(p99));
            Kind::LoadTick(wire::LoadTick { second, users, rps, p50_ms, p95_ms, p99_ms, errors, breaching })
        }
    }
}

/// `kind` under `seq`.
const fn at(seq: u64, kind: Kind) -> wire::RunEvent {
    wire::RunEvent { seq, kind: Some(kind) }
}

/// The run started, by the server's clock (D228); its number in the history is not known yet.
fn run_started(files: usize, instances: usize) -> wire::RunStarted {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let started_ms = u64::try_from(since.as_millis()).unwrap_or(u64::MAX);
    wire::RunStarted { files: count(files), instances: count(instances), run: None, started_ms }
}

/// The instances the run will run, in plan order.
fn run_planned(names: &Names) -> wire::RunPlanned {
    let instance = |planned: &Planned| wire::PlannedInstance {
        instance: planned.instance.index(),
        test: names.test(planned.test).to_owned(),
        label: names.label(planned.test, Some(planned.instance)),
        example_row: planned.example_row,
        repeat_index: planned.repeat_index,
    };
    wire::RunPlanned { instances: names.planned().iter().map(instance).collect() }
}

/// `action` and `owner` as an action event carries them.
fn scope(names: &Names, action: ActionId, owner: OwnerKey) -> Scope {
    Scope { action: Some(action_ref(names, action)), owner: Some(owner_of(names, owner)) }
}

/// `action` with its label and, when it calls an api, the api with its method and path.
fn action_ref(names: &Names, action: ActionId) -> wire::ActionRef {
    let (method, path) = action_call(names, action).map(|call| (call.method, call.path)).unzip();
    wire::ActionRef { index: action.index(), label: names.action(action), api: names.api_of(action), method, path }
}

/// `owner` with its label.
fn owner_of(names: &Names, owner: OwnerKey) -> wire::Owner {
    let kind = match owner {
        OwnerKey::Run => owner::Kind::Run(wire::RunScope {}),
        OwnerKey::File(file) => owner::Kind::File(names.file(file).to_owned()),
        OwnerKey::Instance(instance) => owner::Kind::Instance(instance.index()),
    };
    wire::Owner { label: names.owner(owner), kind: Some(kind) }
}

/// An action started.
fn started(scope: Scope) -> wire::ActionStarted {
    wire::ActionStarted { action: scope.action, owner: scope.owner }
}

/// An action finished, with its failure when it failed.
fn finished(names: &Names, scope: Scope, failed: Option<&Failure>) -> wire::ActionFinished {
    let failure = failed.map(|found| wire_results::failure(names, found));
    wire::ActionFinished { action: scope.action, owner: scope.owner, failure }
}

/// An action's exchange.
fn exchanged(scope: Scope, exchange: results::Exchange) -> wire::Exchanged {
    wire::Exchanged { action: scope.action, owner: scope.owner, exchange: Some(exchange) }
}

/// An action's check.
fn evaluated(scope: Scope, check: results::Check) -> wire::CheckEvaluated {
    wire::CheckEvaluated { action: scope.action, owner: scope.owner, check: Some(check) }
}

/// An action's warning.
fn warned(scope: Scope, warning: results::Failure) -> wire::Warned {
    wire::Warned { action: scope.action, owner: scope.owner, warning: Some(warning) }
}

/// A cleanup that ran, with its failure when it failed.
fn cleanup_finished(names: &Names, cleanup: &CleanupRecord) -> wire::CleanupFinished {
    let ended = finished(names, scope(names, cleanup.action, cleanup.owner), cleanup.outcome.failure());
    wire::CleanupFinished { action: ended.action, owner: ended.owner, failure: ended.failure }
}

/// The cleanups `owner` left unrun.
fn abandoned(names: &Names, owner: OwnerKey, cleanups: &[ActionId]) -> wire::CleanupsAbandoned {
    let cleanups = cleanups.iter().map(|action| action_ref(names, *action)).collect();
    wire::CleanupsAbandoned { owner: Some(owner_of(names, owner)), cleanups }
}

/// The run finished.
fn run_finished(status: RunStatus, sum: &Totals) -> wire::RunFinished {
    wire::RunFinished { status: run_status(status).into(), totals: Some(totals(sum)) }
}

/// How the run ended, as `vc run` reports it.
fn run_report(report: RunReport) -> wire::RunReport {
    wire::RunReport { exit: u32::from(report.exit), seed: report.seed, notices: report.notices, run: report.run }
}

/// `n`, saturating at `u32::MAX`.
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `duration` in milliseconds.
fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
