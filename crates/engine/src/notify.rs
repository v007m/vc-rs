//! Sends the `alerts` notices when a run ends. A channel that cannot be built or does not take
//! the notice is a V5117 notice; it never changes the run's result.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use vc_diagnostics::Code;
use vc_interfaces::{Environment, HistoryReader, Notice, NotifierFactory, RunId, RunSummary, Secrets};
use vc_model::{AlertChannel, CelProgram, Namespace, Program, Reference, RunStatus, Segment};
use vc_template::{Chance, PipeRegistry, RenderError, Renderer, Resolve};

use crate::alerts::{self, Ending, Finished};
use crate::plan::Prepared;
use crate::{Engine, EngineError};

/// How many stored runs are searched for the previous one.
const LOOKBACK: usize = 50;

/// What a run's end needs to send its alerts.
pub struct Alerting {
    program: Arc<Program>,
    project: String,
    root: PathBuf,
    previous: Option<RunStatus>,
    senders: Senders,
}

/// Builds each channel's notifier from its settings, `${secret.…}` and `${env.…}` filled in.
struct Senders {
    factory: Arc<dyn NotifierFactory>,
    secrets: Secrets,
    environment: Arc<dyn Environment>,
}

impl Engine {
    /// What the end of run `run` of `prepared` sends its alerts with; `None` when the config
    /// has no `alerts`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Internal`] when the HTTP client cannot be built.
    pub(crate) fn alerting(
        &self,
        prepared: &Prepared,
        secrets: &Secrets,
        run: u64,
    ) -> Result<Option<Alerting>, EngineError> {
        let program = Arc::clone(&prepared.program);
        if program.config.alerts.is_empty() {
            return Ok(None);
        }
        let factory = self.notifiers(self.transport(prepared.environment.as_ref())?);
        let environment = Arc::clone(&prepared.environment);
        let senders = Senders { factory, secrets: secrets.clone(), environment };
        let previous = previous(self.history_reader().as_ref(), run, &prepared.names);
        let project = program.config.project.clone().unwrap_or_else(|| folder(self.root()));
        Ok(Some(Alerting { program, project, root: self.root().to_path_buf(), previous, senders }))
    }
}

/// The name of the folder `root` is.
fn folder(root: &Path) -> String {
    root.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// How the last finished `vc run` under the same profiles before run `run` ended.
fn previous(history: &dyn HistoryReader, run: u64, profiles: &[String]) -> Option<RunStatus> {
    let runs = history.runs(LOOKBACK).ok()?;
    let earlier = |row: &&vc_interfaces::RunRow| row.id != RunId::new(run) && row.start.kind == "run";
    runs.iter().filter(earlier).filter(|row| row.start.profiles == profiles).find_map(|row| row.status)
}

/// Sends the notices of the rules that fire for `run`, which ended as `ending` says: a
/// V5117 line for each channel that failed.
pub async fn send(alerting: Option<Alerting>, run: RunSummary, ending: Ending) -> Vec<String> {
    let Some(alerting) = alerting else { return Vec::new() };
    let report =
        ending.report.map(|path| path.strip_prefix(&alerting.root).map_or_else(|_| path.clone(), Path::to_path_buf));
    let finished = Finished {
        project: alerting.project.clone(),
        summary: run,
        tests: ending.tests,
        breaches: Vec::new(),
        report: report.map(|path| path.to_string_lossy().replace('\\', "/")),
    };
    let mut problems = Vec::new();
    for fired in alerts::decide(&alerting.program.config.alerts, &finished, alerting.previous) {
        for channel in &fired.rule.notify {
            if let Err(reason) = alerting.senders.send(&alerting.program, channel, &fired.notice).await {
                problems.push(format!("{}: {reason}", Code::V5117));
            }
        }
    }
    problems
}

impl Senders {
    /// Sends `notice` to `channel`: why it was not sent, otherwise.
    async fn send(&self, program: &Program, channel: &AlertChannel, notice: &Notice) -> Result<(), String> {
        let settings = self.settings(program, channel)?;
        let notifier = self.factory.create(&channel.kind, &settings).map_err(|error| error.to_string())?;
        notifier.notify(notice).await.map_err(|error| error.to_string())
    }

    /// `channel`'s settings with their templates filled in.
    fn settings(&self, program: &Program, channel: &AlertChannel) -> Result<Value, String> {
        let pipes = PipeRegistry::builtin();
        let lookup = Lookup { senders: self, program };
        let rendered = Renderer::new(&pipes, &lookup).value(&channel.settings);
        rendered.map_err(|error| format!("the `{}` alert channel's settings: {error}", channel.kind))
    }
}

/// Answers the lookups of a channel's settings: `secret` and `env` only.
struct Lookup<'l> {
    senders: &'l Senders,
    program: &'l Program,
}

impl Lookup<'_> {
    fn env(&self, name: &str) -> Result<Value, RenderError> {
        if self.program.config.secrets.declares(name) {
            let reason = format!("`env.{name}` is a declared secret; use `${{secret.{name}}}` so it is masked");
            return Err(RenderError::invalid(reason));
        }
        let value = self.senders.environment.var(name);
        value.map(Value::String).ok_or_else(|| RenderError::missing(format!("`env.{name}` is not set")))
    }

    fn secret(&self, name: &str) -> Result<Value, RenderError> {
        let secret = self.senders.secrets.get(name);
        let missing = || RenderError::missing(format!("`secret.{name}` is not loaded"));
        secret.map(|secret| Value::String(secret.expose().to_owned())).ok_or_else(missing)
    }
}

impl Resolve for Lookup<'_> {
    fn resolve(&self, reference: &Reference) -> Result<Value, RenderError> {
        let (root, name, rest) = match (&reference.ns, reference.path.segments.split_first()) {
            (Namespace::Env, Some((Segment::Key(name), rest))) => ("env", self.env(name)?, rest),
            (Namespace::Secret, Some((Segment::Key(name), rest))) => ("secret", self.secret(name)?, rest),
            _ => return Err(RenderError::invalid("alert settings read only `${secret.…}` and `${env.…}`")),
        };
        vc_template::select(root, &name, rest)
    }

    fn evaluate(&self, _program: &CelProgram) -> Result<Value, RenderError> {
        Err(RenderError::invalid("alert settings read only `${secret.…}` and `${env.…}`"))
    }

    fn chance(&self) -> &dyn Chance {
        &Steady
    }
}

/// The random source of settings, which have no random values.
struct Steady;

impl Chance for Steady {
    fn next_u64(&self) -> u64 {
        0
    }
}

#[cfg(test)]
#[path = "notify_tests.rs"]
mod tests;
