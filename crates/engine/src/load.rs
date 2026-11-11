//! `vc load <scenario>`: a load scenario from a `*.load.yaml` file, run against its profile.

use std::sync::Arc;
use std::time::Duration;

use vc_diagnostics::Code;
use vc_executor::{CancelTree, EventBus, ExecutorError, IterationSetup, ProgramIterations, RunOptions};
use vc_interfaces::{Random, RunEnd, RunStart, RunSummary};
use vc_loadtest::{Ending, LoadError, LoadOutcome};
use vc_model::{ActionId, ActionKind, LoadScenario, Program, RunStatus, Totals};

use crate::load_live::{self, Live};
use crate::load_report::{self, LoadBreach, LoadEnding};
use crate::notify::{self, Alerting};
use crate::plan::{Extra, Prepared};
use crate::run::executor_error;
use crate::{Engine, EngineError, LoadReport, LoadRequest, LoadWatch, PlanRequest, SelectRequest, alerts, history};

/// The kind a test run is stored and alerted as.
pub const RUN_KIND: &str = "run";
/// The kind a load run is stored and alerted as.
pub const LOAD_KIND: &str = "load";

/// A load run between its plan and its report.
struct Session {
    prepared: Prepared,
    scenario: LoadScenario,
    seed: u64,
    random: Arc<dyn Random>,
    /// The run's root: Ctrl-C cancels it.
    root: CancelTree,
    started_ms: u64,
    report: LoadReport,
    /// What the run's end sends its alerts with.
    alerting: Option<Alerting>,
}

impl Engine {
    /// Runs the scenario's stages against the program's tests, then judges its thresholds on
    /// the whole run. `beforeRun` and each mixed file's `beforeAll` run once, first; each closed
    /// second is written to the history and sent to `watch.ticks`; each value of
    /// `watch.interrupts` is one Ctrl-C. Call it from within a Tokio runtime.
    ///
    /// # Errors
    ///
    /// The errors of [`Engine::plan`]; [`EngineError::Selection`] when no load file has the
    /// scenario's ID, a threshold cannot be judged or the plan reaches an action this build
    /// cannot run; [`EngineError::Setting`] when the declared secrets cannot be loaded.
    pub async fn load(&self, request: &LoadRequest, watch: LoadWatch) -> Result<LoadReport, EngineError> {
        Box::pin(self.load_run(request, watch)).await
    }

    /// [`Engine::load`]'s work, boxed there: a load run's future is large.
    async fn load_run(&self, request: &LoadRequest, mut watch: LoadWatch) -> Result<LoadReport, EngineError> {
        let mut session = self.load_session(request)?;
        let secrets = self.secrets(&session.prepared.program.config.secrets, &session.prepared.environment).await?;
        let live = Live::new(Arc::clone(&session.prepared.program), self.load_recording(&mut session), watch.ticks);
        let run_id = live.run_id().unwrap_or(0);
        session.alerting = self.alerting(&session.prepared, &secrets, (run_id, LOAD_KIND))?;
        let plugins = self.plugin_hosts(&session.prepared.program, &secrets);
        let reached = crate::body_files::reached(&session.prepared.program, &session.prepared.plan);
        let inputs = self.inputs(&session.prepared, (Arc::clone(&session.random), run_id, reached)).await;
        let inputs = inputs.with_secrets(secrets).with_started_at(session.started_ms);
        let cx = Arc::new(self.action_cx(&session.prepared, (inputs, &plugins))?);
        let full_checks = session.scenario.full_checks;
        let setup = IterationSetup { bus: EventBus::spawn(), cancel: session.root.clone(), full_checks };
        let prepared = ProgramIterations::prepare(cx, Arc::clone(&session.prepared.plan), setup);
        let (outcome, live) = match load_live::forwarding(prepared, &mut watch.interrupts, &session.root).await {
            Ok(iterations) => {
                let measured = load_live::measure(&session.scenario, (Arc::new(iterations), live), &session.random);
                let (outcome, live) = load_live::forwarding(measured, &mut watch.interrupts, &session.root).await;
                (outcome.map(Some), live)
            }
            Err(ExecutorError::SharedFailed { action, message }) => {
                let message = shared_failure(&session.prepared.program, action, &message);
                (Ok(blocked(&mut session, message)), live)
            }
            Err(error) => return Err(executor_error(&error)),
        };
        let closed = self.load_close(session, outcome, live).await;
        plugins.shutdown().await;
        closed
    }

    /// Compiles the project, finds `request`'s scenario and plans its mix tests.
    fn load_session(&self, request: &LoadRequest) -> Result<Session, EngineError> {
        let (program, _) = self.program()?;
        let scenario = scenario(&program, &request.scenario)?;
        let ids = scenario.mix.iter().filter_map(|entry| program.str(entry.test.sym()).map(str::to_owned)).collect();
        let select = SelectRequest { ids, ..SelectRequest::default() };
        let plan = PlanRequest { select, profiles: request.profiles.clone(), sets: request.sets.clone(), repeat: None };
        let prepared = self.prepare(&plan, Extra::default())?;
        let (seed, random) = (prepared.draw.seed, Arc::clone(&prepared.draw.random));
        let (name, id) = (scenario.name.clone(), request.scenario.clone());
        let report = LoadReport { scenario: id, name, seed, ..LoadReport::default() };
        let (root, started_ms) = (CancelTree::new(RunOptions::default().grace), self.clock.now_ms());
        Ok(Session { prepared, scenario, seed, random, root, started_ms, report, alerting: None })
    }

    /// Starts recording the load run; a history that cannot be opened leaves it unrecorded,
    /// with a notice saying so.
    fn load_recording(&self, session: &mut Session) -> Option<history::Recording> {
        let start = RunStart {
            kind: LOAD_KIND.to_owned(),
            started_ms: session.started_ms,
            profiles: session.prepared.names.clone(),
            selector: session.report.scenario.clone(),
            seed: Some(session.seed),
            vc_version: env!("CARGO_PKG_VERSION").to_owned(),
            ..RunStart::default()
        };
        self.recording(&start)
            .map_err(|error| session.report.notices.push(format!("history: {error}; this run is not recorded")))
            .ok()
    }

    /// Closes the history's run as `outcome` ended and reports it.
    async fn load_close(
        &self,
        mut session: Session,
        outcome: Result<Option<LoadOutcome>, LoadError>,
        mut live: Live,
    ) -> Result<LoadReport, EngineError> {
        let mut report = std::mem::take(&mut session.report);
        report.notices.splice(0..0, session.prepared.notices.drain(..));
        report.run_id = live.run_id();
        let status = match &outcome {
            Ok(Some(outcome)) => load_report::fill(&mut report, &session.prepared.program, outcome),
            Ok(None) | Err(_) => report.status(),
        };
        report.error_kinds = live.error_kinds(&session.prepared.program);
        report.notices.extend(live.problem.take());
        let (totals, finished_ms) = (iterations(&report), self.clock.now_ms());
        let end = RunEnd { status, totals, finished_ms, snapshot: Vec::new() };
        if let Some(recording) = live.recording.take()
            && let Err(error) = recording.close(end, session.prepared.program.config.history.keep).await
        {
            report.notices.push(format!("history: {error}"));
        }
        if outcome.is_ok() {
            let sent = self.load_alerts(&mut session, &report, status).await;
            report.notices.extend(sent);
        }
        outcome.map(|_| report).map_err(|error| load_error(&error))
    }

    /// Sends the alerts of the load run `report` describes, which ended as `status`: `threshold`
    /// with the thresholds breached; `recovery` once a run that recorded iterations breaches none.
    async fn load_alerts(&self, session: &mut Session, report: &LoadReport, status: RunStatus) -> Vec<String> {
        let elapsed = Duration::from_millis(self.clock.now_ms().saturating_sub(session.started_ms));
        let run = RunSummary::new(status, Totals::default()).timed(session.started_ms, elapsed);
        let run = run.profiles(session.prepared.names.clone());
        let breaches = report.breaches.iter().map(breach).collect();
        let ending = alerts::Ending { breaches, ran: report.iterations > 0, ..alerts::Ending::default() };
        notify::send(session.alerting.take(), run, ending).await
    }
}

/// Marks `session`'s report blocked by a shared action's failure, or cancelled when Ctrl-C cut
/// it; no user started.
fn blocked(session: &mut Session, message: String) -> Option<LoadOutcome> {
    let report = &mut session.report;
    if session.root.is_cancelled() {
        (report.ending, report.exit) = (LoadEnding::Cancelled, 130);
    } else {
        (report.ending, report.exit, report.blocked) = (LoadEnding::Blocked, 1, Some(message));
    }
    None
}

/// `V5106: the shared action `seed` failed: …`, the action named by its `id`, else by the api
/// it calls, as a functional run's blocked tests name it.
fn shared_failure(program: &Program, action: ActionId, message: &str) -> String {
    let found = usize::try_from(action.index()).ok().and_then(|at| program.actions.get(at));
    let name = found.and_then(|found| match (found.id, &found.kind) {
        (Some(id), _) => program.str(id).map(str::to_owned),
        (None, ActionKind::Call(api)) => program.str(api.sym()).map(|api| format!("call {api}")),
        _ => None,
    });
    let name = name.unwrap_or_else(|| format!("action {}", action.index()));
    format!("{}: the shared action `{name}` failed: {message}", Code::V5106)
}

/// A load run's totals for the history: its iterations, as passed and failed.
fn iterations(report: &LoadReport) -> Totals {
    let count = |iterations: u64| usize::try_from(iterations).unwrap_or(usize::MAX);
    let passed = count(report.iterations.saturating_sub(report.errors));
    Totals { passed, failed: count(report.errors), ..Totals::default() }
}

/// How an alert names `breached`: its metric, and the test whose threshold it is.
fn breach(breached: &LoadBreach) -> String {
    breached.test.as_ref().map_or_else(|| breached.metric.clone(), |test| format!("{} of {test}", breached.metric))
}

/// The scenario whose load file has the ID `id`.
fn scenario(program: &Program, id: &str) -> Result<LoadScenario, EngineError> {
    if let Some(found) = program.loads.values().find(|scenario| program.str(scenario.id.sym()) == Some(id)) {
        return Ok(found.clone());
    }
    let known: Vec<&str> = program.loads.values().filter_map(|scenario| program.str(scenario.id.sym())).collect();
    let help = if known.is_empty() {
        "add a load file, such as `load/checkout/peak.load.yaml` (scenario `checkout.peak`)".to_owned()
    } else {
        format!("the scenarios are: {}", known.join(", "))
    };
    Err(EngineError::Selection { reason: format!("no load scenario has the ID `{id}`"), help: Some(help) })
}

/// A scenario that cannot be judged or picked from is the user's to fix; anything else is a bug.
fn load_error(error: &LoadError) -> EngineError {
    match error {
        LoadError::EmptyMix | LoadError::Threshold { .. } => {
            EngineError::Selection { reason: error.to_string(), help: None }
        }
        _ => EngineError::Internal { reason: error.to_string() },
    }
}

/// The exit code (D127) and history status of a load run that ended as `ending` with
/// `breached` thresholds over the whole run: 130 cancelled, 3 breached, else 0.
pub const fn exit(ending: Ending, breached: bool) -> (u8, RunStatus) {
    match (ending, breached) {
        (Ending::Cancelled, _) => (130, RunStatus::Cancelled),
        (_, true) => (3, RunStatus::Failed),
        _ => (0, RunStatus::Passed),
    }
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;
