//! `vc call <apiId>`: one api sent ad hoc with the project's auth and the active profiles
//! (D178). No plan runs, so `beforeRun` and `beforeAll` are skipped; the api's cleanup runs
//! afterwards unless `--keep`, and the call is recorded in the history as `adhoc`.

use std::iter;
use std::sync::Arc;

use serde_json::{Map, Value, json};
use vc_executor::{ActionCx, ActionReport, ActionRequest, Frame, Saved};
use vc_interfaces::{ConfigEntry, RunEnd, RunStart};
use vc_model::{
    Action, ActionId, ActionKind, ActionOutcome, Api, Names, Owner, Program, RunStatus, SaveSpec, TestStatus, Totals,
    VarDefs,
};

use crate::agent_gate::{Gated, call_reach};
use crate::call_report::CallReport;
use crate::examples::Draw;
use crate::explain::did_you_mean;
use crate::history::Recording;
use crate::plan::{self, Prepared, profile_flag};
use crate::policy::AgentCall;
use crate::ports::Layered;
use crate::{CallRequest, Engine, EngineError};

/// The history kind of an ad hoc call.
const KIND: &str = "adhoc";

/// The tool an agent's call is judged and logged as.
const CALL_TOOL: &str = "call_api";

impl Engine {
    /// Sends the api `request.api` once, runs its cleanup unless `request.keep`, and records
    /// the call in the history.
    ///
    /// # Errors
    ///
    /// [`EngineError::Selection`] when no api has that ID, [`EngineError::Setting`] for a
    /// `--var` that is not `name=value` or a bad setting, and the errors of loading the project.
    pub async fn call(&self, request: &CallRequest) -> Result<CallReport, EngineError> {
        let (given, mut prepared, action) =
            self.before_gate(agent_call(request).as_ref(), self.adhoc_call(request)).await?;
        let admitted = self.gate(request, &prepared.program, action).await?;
        let secrets = self.secrets(&prepared.program.config.secrets, &prepared.environment).await?;
        let mut notices = std::mem::take(&mut prepared.notices);
        let recording = self.call_recording(&prepared, &request.api, &mut notices);
        let run_id = recording.as_ref().map_or(0, |recording| recording.id().get());
        let cx = self.adhoc_cx(&prepared, (&[action], run_id, secrets)).await?;
        let (report, failed) = sent_once(&cx, action, given).await;
        notices.extend(failed);
        let passed = matches!(report.outcome, ActionOutcome::Passed);
        notices.extend(self.closed(recording, passed, &prepared.program).await);
        self.settle(admitted, passed).await?;
        let names = Names::new(Arc::clone(&prepared.program), &prepared.plan);
        let mut called = CallReport::masked(&cx, (names, action), report).await;
        called.notices = notices;
        Ok(called)
    }

    /// For an agent's call, the agent policy's gate on what `action` reaches: the api's method,
    /// its cleanup's unless `keep`, and what its `flow` auth sends and runs. Gives the call as
    /// the log records it, and who approved it; `None` for a person's call, which is not gated.
    ///
    /// # Errors
    ///
    /// [`EngineError::Refused`] when the policy refuses it or no one approves it in time.
    async fn gate(
        &self,
        request: &CallRequest,
        program: &Program,
        action: ActionId,
    ) -> Result<Option<Gated>, EngineError> {
        let Some(call) = agent_call(request) else { return Ok(None) };
        let reached = call_reach(program, action, request.keep);
        let summary = format!("{} {}", reached.methods.first().map_or("GET", |method| method.name()), request.api);
        self.gated(AgentCall { summary, methods: reached.methods, code: reached.code, ..call }).await.map(Some)
    }

    /// The `--var` values, the settings and the api's action, all before the gate.
    fn adhoc_call(&self, request: &CallRequest) -> Result<(Saved, Prepared, ActionId), EngineError> {
        let given = given(&request.vars)?;
        let mut prepared = self.adhoc(request)?;
        let action = adhoc_action(&mut prepared.program, &request.api, request.keep)?;
        Ok((given, prepared, action))
    }

    /// The layered settings and active profiles of `request`, with nothing planned.
    pub(crate) fn adhoc(&self, request: &CallRequest) -> Result<Prepared, EngineError> {
        let Layered { mut program, notices, settings, environment, dot_env, .. } = self.call_layered(request)?;
        settings.apply(&mut program);
        let (profiles, names) = plan::profiles(&program, settings.profiles.as_ref())?;
        let (plan, draw) = (Arc::default(), Draw::new(None));
        Ok(Prepared { program, plan, settings, profiles, names, notices, environment, dot_env, draw })
    }

    /// The settings `request` layers, under its profiles, with its draft compiled in place of
    /// the file it edits.
    pub(crate) fn call_layered(&self, request: &CallRequest) -> Result<Layered, EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(&request.profiles).into_iter().collect();
        let draft = request.draft.iter().map(|draft| self.draft_change(draft)).collect::<Result<Vec<_>, _>>()?;
        self.layered_with(&request.sets, &flags, &draft)
    }

    /// The history entry of the call, or `None` and a notice that it is not recorded.
    fn call_recording(&self, prepared: &Prepared, api: &str, notices: &mut Vec<String>) -> Option<Recording> {
        let start = RunStart {
            kind: KIND.to_owned(),
            started_ms: self.clock.now_ms(),
            profiles: prepared.names.clone(),
            selector: api.to_owned(),
            vc_version: env!("CARGO_PKG_VERSION").to_owned(),
            ..RunStart::default()
        };
        self.recording(&start)
            .map_err(|error| notices.push(format!("history: {error}; this call is not recorded")))
            .ok()
    }

    /// Closes the call's history record, when it has one; a notice when that fails.
    async fn closed(&self, recording: Option<Recording>, passed: bool, program: &Program) -> Option<String> {
        let recording = recording?;
        let mut totals = Totals::default();
        totals.add(if passed { TestStatus::Passed } else { TestStatus::Failed });
        let status = if passed { RunStatus::Passed } else { RunStatus::Failed };
        let end = RunEnd { status, totals, finished_ms: self.clock.now_ms(), snapshot: Vec::new() };
        let closed = recording.close(end, program.config.history.keep).await;
        closed.err().map(|error| format!("history: {error}; this call is not recorded"))
    }
}

/// `--var name=value`, each as a string value.
/// An agent's call as the log names it before the api is found; `None` for a person's call.
fn agent_call(request: &CallRequest) -> Option<AgentCall> {
    let agent = request.agent.as_ref()?;
    let vars: Map<String, Value> = request
        .vars
        .iter()
        .filter_map(|var| var.split_once('='))
        .map(|(name, value)| (name.to_owned(), Value::String(value.to_owned())))
        .collect();
    Some(AgentCall {
        agent: agent.clone(),
        tool: CALL_TOOL.to_owned(),
        profiles: request.profiles.clone(),
        summary: request.api.clone(),
        arguments: json!({"api": request.api, "vars": vars, "keep": request.keep}),
        ..AgentCall::default()
    })
}

/// The `name=value` vars of `vars`, as a call's own; a bad one is named by its place, since it
/// may be a value that is not to be echoed.
pub fn given(vars: &[String]) -> Result<Saved, EngineError> {
    let mut given = Saved::new();
    for (at, var) in vars.iter().enumerate() {
        let Some((name, value)) = var.split_once('=').filter(|(name, _)| !name.is_empty()) else {
            return Err(EngineError::Setting {
                origin: "--var".to_owned(),
                reason: format!("var {} is not `name=value`", at + 1),
                help: Some("write it as `--var userId=7`".to_owned()),
            });
        };
        given.insert(name.to_owned(), Value::String(value.to_owned()));
    }
    Ok(given)
}

/// Adds a run-level action that calls the api `id` with the api's own expect, and returns it.
fn adhoc_action(program: &mut Arc<Program>, id: &str, keep: bool) -> Result<ActionId, EngineError> {
    let api = program.apis.values().find(|api| program.str(api.id.sym()) == Some(id));
    let action = api.map(|api| called(api, keep)).ok_or_else(|| unknown(program, id))?;
    let program = Arc::make_mut(program);
    let index = u32::try_from(program.actions.len())
        .map_err(|_| EngineError::Internal { reason: "the program has too many actions".to_owned() })?;
    program.actions.push(action);
    Ok(ActionId::new(index))
}

fn called(api: &Api, keep: bool) -> Action {
    Action {
        id: None,
        kind: ActionKind::Call(api.id),
        owner: Owner::Run,
        vars: VarDefs::default(),
        auth: None,
        headers: iter::empty().collect(),
        query: iter::empty().collect(),
        body: None,
        expect: api.expect.clone(),
        save: SaveSpec::default(),
        poll: None,
        retry: None,
        timeout: None,
        keep,
        provides: Vec::new(),
        cache: None,
        repeat: None,
        span: api.span,
    }
}

pub fn unknown(program: &Program, id: &str) -> EngineError {
    let names = program.apis.keys().filter_map(|api| program.str(api.sym()));
    EngineError::Selection { reason: format!("no api `{id}`"), help: did_you_mean(&vc_planner::closest(id, names)) }
}

/// Sends `action` once with the `--var` values `given`, then runs the cleanups it registered;
/// gives its report and a notice for each cleanup that failed.
async fn sent_once(cx: &Arc<ActionCx>, action: ActionId, given: Saved) -> (ActionReport, Vec<String>) {
    let mut sent = ActionRequest::cleanup(cx, action, &Arc::new(Saved::new()));
    sent.frame = Frame::default();
    sent.frame.saved = Arc::new(given);
    let report = cx.executors.execute(sent).await;
    let failed = clean(cx, &report).await;
    (report, failed)
}

/// Runs the cleanups the call registered, last first; each failure is a notice.
async fn clean(cx: &Arc<ActionCx>, report: &ActionReport) -> Vec<String> {
    let mut notices = Vec::new();
    for cleanup in report.cleanups.iter().rev() {
        let done = cx.executors.execute(ActionRequest::cleanup(cx, cleanup.action, &cleanup.vars)).await;
        if let ActionOutcome::Failed(failure) = done.outcome {
            notices.push(format!("cleanup failed: {}", cx.redactor.masked_text(&failure.message).await));
        }
    }
    notices
}

#[cfg(test)]
#[path = "call_tests.rs"]
mod tests;
