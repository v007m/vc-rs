//! `vc call <apiId>`: one api sent ad hoc with the project's auth and the active profiles
//! (D178). No plan runs, so `beforeRun` and `beforeAll` are skipped; the api's cleanup runs
//! afterwards unless `--keep`, and the call is recorded in the history as `adhoc`.

use std::fmt::Write as _;
use std::iter;
use std::sync::Arc;

use serde_json::Value;
use vc_executor::{ActionCx, ActionReport, ActionRequest, Frame, Saved};
use vc_interfaces::{ConfigEntry, RunEnd, RunStart};
use vc_model::{
    Action, ActionId, ActionKind, ActionOutcome, Api, Owner, Program, RunStatus, SaveSpec, TestStatus, Totals, VarDefs,
};

use crate::explain::did_you_mean;
use crate::history::Recording;
use crate::plan::{self, Prepared, profile_flag};
use crate::ports::{self, Layered};
use crate::{CallRequest, Engine, EngineError, run};

/// The history kind of an ad hoc call.
const KIND: &str = "adhoc";

/// What `vc call` got back.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CallReport {
    /// 0 when the api's expect held, 1 when it did not or nothing came back.
    pub exit: u8,
    /// `200 OK · 148 ms · api expect passed`, the body, and the failure when there is one.
    pub text: String,
    /// What the run should still say: settings notices, failed cleanups, an unrecorded history.
    pub notices: Vec<String>,
    /// The response as the checks saw it (`status`, `headers`, `body`), masked; what
    /// `--save-as-test` takes its expectations from.
    pub response: Option<Value>,
}

impl Engine {
    /// Sends the api `request.api` once, runs its cleanup unless `request.keep`, and records
    /// the call in the history.
    ///
    /// # Errors
    ///
    /// [`EngineError::Selection`] when no api has that ID, [`EngineError::Setting`] for a
    /// `--var` that is not `name=value` or a bad setting, and the errors of loading the project.
    pub async fn call(&self, request: &CallRequest) -> Result<CallReport, EngineError> {
        let given = given(&request.vars)?;
        let mut prepared = self.adhoc(request)?;
        let action = adhoc_action(&mut prepared.program, &request.api, request.keep)?;
        let secrets = self.secrets(&prepared.program.config.secrets, &prepared.environment).await?;
        let mut notices = std::mem::take(&mut prepared.notices);
        let recording = self.call_recording(&prepared, &request.api, &mut notices);
        let run_id = recording.as_ref().map_or(0, |recording| recording.id().get());
        let inputs = run::inputs(&prepared, ports::random(None).1, run_id);
        let inputs = inputs.with_secrets(secrets).with_started_at(self.clock.now_ms());
        let cx = Arc::new(ActionCx::new(Arc::clone(&prepared.program), self.executors(&prepared)?, inputs));
        let mut sent = ActionRequest::cleanup(&cx, action, &Arc::new(Saved::new()));
        sent.frame = Frame::default();
        sent.frame.saved = Arc::new(given);
        let report = cx.executors.execute(sent).await;
        notices.extend(clean(&cx, &report).await);
        let passed = matches!(report.outcome, ActionOutcome::Passed);
        if let Some(recording) = recording {
            notices.extend(self.closed(recording, passed, &prepared.program).await);
        }
        let response = report.response.as_deref().map(|response| cx.redactor.value(response.clone()));
        Ok(CallReport { exit: u8::from(!passed), text: shown(&cx, &report), notices, response })
    }

    /// The layered settings and active profiles of `request`, with nothing planned.
    fn adhoc(&self, request: &CallRequest) -> Result<Prepared, EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(&request.profiles).into_iter().collect();
        let Layered { mut program, notices, settings, environment, dot_env, .. } =
            self.layered(&request.sets, &flags)?;
        if let Some(timeout) = settings.timeout {
            Arc::make_mut(&mut program).config.defaults.timeout = Some(timeout);
        }
        let (profiles, names) = plan::profiles(&program, settings.profiles.as_ref())?;
        Ok(Prepared { program, plan: Arc::default(), settings, profiles, names, notices, environment, dot_env })
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

    async fn closed(&self, recording: Recording, passed: bool, program: &Program) -> Option<String> {
        let mut totals = Totals::default();
        totals.add(if passed { TestStatus::Passed } else { TestStatus::Failed });
        let status = if passed { RunStatus::Passed } else { RunStatus::Failed };
        let end = RunEnd { status, totals, finished_ms: self.clock.now_ms(), snapshot: Vec::new() };
        let closed = recording.close(end, program.config.history.keep).await;
        closed.err().map(|error| format!("history: {error}; this call is not recorded"))
    }
}

/// `--var name=value`, each as a string value.
fn given(vars: &[String]) -> Result<Saved, EngineError> {
    let mut given = Saved::new();
    for var in vars {
        let Some((name, value)) = var.split_once('=').filter(|(name, _)| !name.is_empty()) else {
            return Err(EngineError::Setting {
                origin: "--var".to_owned(),
                reason: format!("`{var}` is not `name=value`"),
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
        keep,
        cache: None,
        repeat: None,
        span: api.span,
    }
}

fn unknown(program: &Program, id: &str) -> EngineError {
    let names = program.apis.keys().filter_map(|api| program.str(api.sym()));
    EngineError::Selection { reason: format!("no api `{id}`"), help: did_you_mean(&vc_planner::closest(id, names)) }
}

/// Runs the cleanups the call registered, last first; each failure is a notice.
async fn clean(cx: &Arc<ActionCx>, report: &ActionReport) -> Vec<String> {
    let mut notices = Vec::new();
    for cleanup in report.cleanups.iter().rev() {
        let done = cx.executors.execute(ActionRequest::cleanup(cx, cleanup.action, &cleanup.vars)).await;
        if let ActionOutcome::Failed(failure) = done.outcome {
            notices.push(format!("cleanup failed: {}", cx.redactor.text(&failure.message)));
        }
    }
    notices
}

/// The status line, the masked body, and the failure when the expect did not hold.
fn shown(cx: &ActionCx, report: &ActionReport) -> String {
    let passed = matches!(report.outcome, ActionOutcome::Passed);
    let mut out = String::new();
    if let Some(exchange) = report.exchanges.last() {
        let verdict = if passed { "api expect passed" } else { "api expect failed" };
        let millis = exchange.timing.total.as_millis();
        let _ = writeln!(out, "{} · {millis} ms · {verdict}", exchange.response.status);
        out.push_str(&body(cx, &exchange.response.body));
    }
    if let ActionOutcome::Failed(failure) = &report.outcome {
        let _ = writeln!(out, "✗ {}", cx.redactor.text(&failure.message));
    }
    out
}

/// The body as indented JSON when it is JSON, else as text; masked, with a closing newline.
fn body(cx: &ActionCx, bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let text = serde_json::from_slice::<Value>(bytes).map_or_else(
        |_| cx.redactor.text(&String::from_utf8_lossy(bytes)),
        |json| serde_json::to_string_pretty(&cx.redactor.value(json)).unwrap_or_default(),
    );
    format!("{}\n", text.trim_end())
}

#[cfg(test)]
#[path = "call_tests.rs"]
mod tests;
