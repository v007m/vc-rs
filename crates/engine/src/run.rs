//! `vc run`: plan, execute, and fan the events out to the reporters.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::Receiver;
use vc_executor::{
    ActionCx, ActionExecutors, ActionKindTag, CommandExecutor, Defined, EventBus, Executor, ExecutorError, HostLimits,
    MaskRules, PipelineParts, RunHandle, RunInputs, RunOptions,
};
use vc_interfaces::{ConfigEntry, HookRunner, Random, RunId, RunStart, RunSummary, Transport};
use vc_model::{Feed, Part, Template, TemplateValue};

use crate::cache::Sweep;
use crate::plan::{Extra, Prepared};
use crate::reporting::{self, Reporters, Setup};
use crate::{Engine, EngineError, Interrupts, RunRequest, history, ports};

/// Room for this many events between the bus and the reporters before they miss some.
const FEED: usize = 65_536;
/// Test instances at once, when neither a flag nor the config says.
pub const CONCURRENCY: u32 = 16;
/// Requests at once to one host, when neither a layer nor the config says.
pub const PER_HOST: u32 = 8;

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunReport {
    /// The exit code (D127): 0 passed, 1 failed or blocked, 2 nothing ran, 130 cancelled.
    pub exit: u8,
    /// The seed of the run's random values, for `--seed` to repeat them.
    pub seed: u64,
    /// Problems that did not change the result, such as a report that could not be written.
    pub notices: Vec<String>,
}

impl Engine {
    /// Runs what `request` selects. The reporters print to stdout and write the report files;
    /// each value of `interrupts` is one Ctrl-C. Call it from within a Tokio runtime.
    ///
    /// # Errors
    ///
    /// The errors of [`Engine::plan`]; [`EngineError::Setting`] when the declared secrets
    /// cannot be loaded; [`EngineError::Reporter`] for a reporter that is unknown,
    /// given to the wrong flag or not built yet; [`EngineError::Selection`] when the plan
    /// reaches an action this build cannot run; with `--failed`, [`EngineError::History`] when
    /// no run is stored and [`EngineError::Selection`] when the last one had nothing to rerun.
    pub async fn run(&self, request: &RunRequest, mut interrupts: Interrupts) -> Result<RunReport, EngineError> {
        let (plan, rerun) = self.rerun(request)?;
        let prepared = self.prepare(&plan, Extra { flags: flags(request), rerun })?;
        let admitted = self.run_gate(request, &prepared).await?;
        let secrets = self.secrets(&prepared.program.config.secrets, &prepared.environment).await?;
        let (seed, random) = ports::random(request.seed);
        let (reporters, unrecorded) = self.reporters(request, &prepared, seed)?;
        let run_id = reporters.run_id().map_or(0, RunId::get);
        let inputs = inputs(&prepared, random, run_id).with_secrets(secrets).with_started_at(self.clock.now_ms());
        let (executor, sweep) = self.executor(&prepared, inputs, request).await?;
        let bus = EventBus::spawn();
        let mut fanout = Fanout { feed: bus.subscribe(FEED), reporters };
        let started = self.clock.now_ms();
        let ran = match executor.start(Arc::clone(&prepared.plan), bus.clone()) {
            Ok(handle) => pump(handle, &mut fanout, &mut interrupts).await,
            Err(error) => Err(executor_error(&error)),
        };
        bus.flush().await;
        fanout.drain().await;
        let swept = sweep.finish(&mut interrupts).await;
        let summary = ran?;
        let elapsed = Duration::from_millis(self.clock.now_ms().saturating_sub(started));
        let run =
            RunSummary::new(summary.status, summary.totals).timed(started, elapsed).profiles(prepared.names.clone());
        let mut notices = prepared.notices;
        notices.extend(unrecorded);
        notices.extend(swept);
        notices.extend(fanout.reporters.finish(run).await);
        let exit = vc_executor::exit_code(summary);
        self.settle(admitted, exit == 0)?;
        Ok(RunReport { exit, seed, notices })
    }

    /// The reporters `request` chooses, recording into the history; a history that cannot be
    /// opened leaves the run unrecorded, with a notice saying so.
    fn reporters(
        &self,
        request: &RunRequest,
        prepared: &Prepared,
        seed: u64,
    ) -> Result<(Reporters, Option<String>), EngineError> {
        let chosen = reporting::choose(request.console.as_deref(), &prepared.settings, &prepared.program)?;
        let chosen = chosen.under(self.root()).flat(request.files.out.is_some());
        let selector = history::selector(&request.plan.select);
        let selector = if request.failed { format!("--failed {selector}").trim_end().to_owned() } else { selector };
        let start = RunStart {
            kind: "run".to_owned(),
            started_ms: self.clock.now_ms(),
            profiles: prepared.names.clone(),
            selector,
            seed: Some(seed),
            vc_version: env!("CARGO_PKG_VERSION").to_owned(),
            ..RunStart::default()
        };
        let (recording, notice) = match self.recording(&start) {
            Ok(recording) => (Some(recording), None),
            Err(error) => (None, Some(format!("history: {error}; this run is not recorded"))),
        };
        let headline = format!("plan  {}", prepared.report().line());
        let (program, plan) = (Arc::clone(&prepared.program), Arc::clone(&prepared.plan));
        let setup = Setup { program, plan, headline, color: request.color, seed, quiet: request.agent.is_some() };
        Ok((Reporters::create(chosen, setup, recording)?, notice))
    }

    /// The executor for `prepared` with `inputs`, keeping `cache:`d results in the cross-run
    /// cache unless `request` says `--no-cache`, and what the cache leaves for the run's end.
    async fn executor(
        &self,
        prepared: &Prepared,
        inputs: RunInputs,
        request: &RunRequest,
    ) -> Result<(Executor, Sweep), EngineError> {
        let (settings, defaults) = (&prepared.settings, &prepared.program.config.defaults);
        let concurrency = settings.concurrency.or(defaults.concurrency).unwrap_or(CONCURRENCY);
        let cx = self.action_cx(prepared, inputs)?;
        let sweep = if request.no_cache { Sweep::off(cx, Vec::new()) } else { self.cached(cx, prepared).await };
        let options = RunOptions { concurrency: widen(concurrency), bail: request.bail, ..RunOptions::default() };
        Ok((Executor::new(sweep.cx(), options), sweep))
    }

    /// What the actions of `prepared` share with `inputs`: the action executors and the auth
    /// types over one transport, and events masked by the config's redact rules (a `function:`
    /// rule through the hook runner). Every run, call and cache cleanup starts from it.
    pub(crate) fn action_cx(&self, prepared: &Prepared, inputs: RunInputs) -> Result<ActionCx, EngineError> {
        let hooks = self.hook_runner(&prepared.program);
        let rules = MaskRules::standard().with_hooks(hooks.clone(), &prepared.program);
        let transport = self.transport(prepared.environment.as_ref())?;
        let auth = self.auth(Arc::clone(&transport));
        let executors = self.executors(prepared, transport, hooks);
        Ok(ActionCx::new(Arc::clone(&prepared.program), executors, inputs).with_mask_rules(&rules).with_auth(auth))
    }

    /// The action executors for `prepared`: HTTP through `transport` and the per-host limits,
    /// hooks through `hooks`, `command:` through the command runner.
    fn executors(
        &self,
        prepared: &Prepared,
        transport: Arc<dyn Transport>,
        hooks: Option<Arc<dyn HookRunner>>,
    ) -> Arc<ActionExecutors> {
        let (settings, defaults) = (&prepared.settings, &prepared.program.config.defaults);
        let per_host = settings.per_host.or(defaults.per_host).unwrap_or(PER_HOST);
        let mut parts = PipelineParts::new(transport, HostLimits::spawn(widen(per_host)));
        if let Some(hooks) = hooks {
            parts = parts.with_hook_runner(hooks);
        }
        let root = self.root().to_path_buf();
        let commands = CommandExecutor::new(Arc::clone(&self.commands), root, prepared.dot_env.clone());
        Arc::new(ActionExecutors::standard(parts).with(ActionKindTag::Command, Arc::new(commands)))
    }
}

/// What the run's templates read besides the definitions: the random values, the environment,
/// the active profiles, the `--set vars.*` values and the history's run number. The caller adds
/// the run's start.
pub fn inputs(prepared: &Prepared, random: Arc<dyn Random>, run_id: u64) -> RunInputs {
    RunInputs::new(random, Arc::clone(&prepared.environment))
        .with_profiles(prepared.profiles.clone())
        .with_vars(vars(&prepared.settings.vars))
        .with_run_id(run_id)
}

/// The run flags that are settings, as the layer between `VIB_*` and `--set` (D129).
fn flags(request: &RunRequest) -> Vec<ConfigEntry> {
    let mut flags = Vec::new();
    if let Some(concurrency) = request.concurrency {
        flags.push(ConfigEntry::new("defaults.concurrency", concurrency.to_string(), "--concurrency"));
    }
    if !request.files.formats.is_empty() {
        flags.push(ConfigEntry::new("reports.formats", request.files.formats.join(","), "--report"));
    }
    if request.files.single_file {
        flags.push(ConfigEntry::new("reports.html.layout", "single", "--single-file"));
    }
    if let Some(out) = &request.files.out {
        flags.push(ConfigEntry::new("reports.dir", out.to_string_lossy(), "--out"));
    }
    flags
}

/// The run's event feed and the reporters it fans out to.
struct Fanout {
    feed: Receiver<Feed>,
    reporters: Reporters,
}

impl Fanout {
    /// Hands the reporters the events still queued once the run has finished.
    async fn drain(&mut self) {
        while let Ok(next) = self.feed.try_recv() {
            self.reporters.on(next).await;
        }
    }
}

/// Waits for the run to finish, handing events to the reporters and interrupts to the run as
/// they come.
async fn pump(
    handle: RunHandle,
    fanout: &mut Fanout,
    interrupts: &mut Interrupts,
) -> Result<vc_executor::RunSummary, EngineError> {
    let interrupter = handle.interrupter();
    let finish = handle.finish();
    tokio::pin!(finish);
    loop {
        tokio::select! {
            biased;
            summary = &mut finish => return summary.map_err(|e| executor_error(&e)),
            Some(()) = interrupts.next() => interrupter.interrupt(),
            Some(next) = fanout.feed.recv() => fanout.reporters.on(next).await,
        }
    }
}

/// `--set vars.name=value` as literal config vars.
fn vars(set: &[(String, String)]) -> Defined {
    let literal = |text: &str| TemplateValue::String(Template { parts: vec![Part::Literal(text.to_owned())] });
    set.iter().map(|(name, value)| (name.clone(), literal(value))).collect::<HashMap<_, _>>()
}

fn widen(count: u32) -> usize {
    usize::try_from(count).unwrap_or(usize::MAX)
}

/// An action this build cannot run is the user's to avoid; anything else is a bug.
fn executor_error(error: &ExecutorError) -> EngineError {
    match error {
        ExecutorError::NoExecutor { .. } => EngineError::Selection {
            reason: error.to_string(),
            help: Some("leave the `function:` action out of the selected tests and lifecycle blocks".to_owned()),
        },
        _ => EngineError::Internal { reason: error.to_string() },
    }
}
