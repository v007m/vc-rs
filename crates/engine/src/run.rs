//! `vc run`: plan, execute, fan the events out to the reporters, and send the alerts.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::Receiver;
use vc_executor::{
    ActionCx, ActionExecutors, ActionKindTag, CommandExecutor, Defined, EventBus, Executor, ExecutorError, HostLimits,
    MaskRules, PipelineParts, RunHandle, RunInputs, RunOptions,
};
use vc_interfaces::{ConfigEntry, HookRunner, Random, RunId, RunStart, RunSummary, Transport};
use vc_model::{Feed, Part, RunStatus, Template, TemplateValue, Totals};

use crate::agent_gate::run_call;
use crate::cache::Sweep;
use crate::notify::Alerting;
use crate::plan::{Extra, Prepared};
use crate::plugin_hosts::Hosts;
use crate::reporting::{self, Reporters, Setup};
use crate::run_feed::Watching;
use crate::{Engine, EngineError, Interrupts, RunRequest, history, notify, ports};

/// Room for this many events between the bus and the reporters before they miss some.
const FEED: usize = 65_536;
/// Test instances at once, when neither a flag nor the config says.
pub const CONCURRENCY: u32 = 16;
/// Requests at once to one host, when neither a layer nor the config says.
pub const PER_HOST: u32 = 8;
/// The exit code of an error inside Vibranium (D127).
const EXIT_INTERNAL: u8 = 70;

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
    /// The run's number in the history, when it is recorded.
    pub run: Option<u64>,
}

impl Engine {
    /// Runs what `request` selects. The reporters print to stdout and write the report files,
    /// and `request.watch` gets the live feed, `Done` once the reporters have finished;
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
        let planned =
            self.rerun(request).and_then(|(plan, rerun)| self.prepare(&plan, Extra { flags: flags(request), rerun }));
        let prepared = self.before_gate(run_call(request).as_ref(), planned).await?;
        let admitted = self.run_gate(request, &prepared).await?;
        let report = self.execute(request, prepared, &mut interrupts).await?;
        self.settle(admitted, report.exit == 0).await?;
        Ok(report)
    }

    /// Executes the admitted plan with its secrets and reporters, and gathers its report. A
    /// run the executor fails still finishes its reporters and its watch before the error, and
    /// is reported and recorded as cancelled, not as a failed run with no tests.
    async fn execute(
        &self,
        request: &RunRequest,
        prepared: Prepared,
        interrupts: &mut Interrupts,
    ) -> Result<RunReport, EngineError> {
        let secrets = self.secrets(&prepared.program.config.secrets, &prepared.environment).await?;
        let plugins = self.plugin_hosts(&prepared.program, &secrets);
        let (seed, random) = ports::random(request.seed);
        let (reporters, unrecorded) = self.reporters(request, &prepared, (seed, plugins.clone()))?;
        let run_id = reporters.run_id().map_or(0, RunId::get);
        let alerting = self.alerting(&prepared, &secrets, (run_id, "run"))?;
        let inputs = inputs(&prepared, random, run_id).with_secrets(secrets).with_started_at(self.clock.now_ms());
        let (executor, sweep) = self.executor(&prepared, (inputs, &plugins), request).await?;
        let bus = EventBus::spawn();
        let watching = request.watch.as_ref().map(|watch| watch.follow(&bus, &prepared));
        let mut fanout = Fanout { feed: bus.subscribe(FEED), reporters };
        let started = self.clock.now_ms();
        let ran = pump(executor.start(Arc::clone(&prepared.plan), bus.clone()), &mut fanout, interrupts).await;
        bus.flush().await;
        fanout.drain().await;
        let swept = sweep.finish(interrupts).await;
        let stopped = |_: &EngineError| (RunStatus::Cancelled, Totals::default());
        let (status, totals) = ran.as_ref().map_or_else(stopped, |ran| (ran.status, ran.totals));
        let elapsed = Duration::from_millis(self.clock.now_ms().saturating_sub(started));
        let run = RunSummary::new(status, totals).timed(started, elapsed).profiles(prepared.names.clone());
        let mut notices = prepared.notices;
        notices.extend(unrecorded);
        notices.extend(swept);
        let closing = Closing { seed, run: Some(run_id).filter(|id| *id > 0), notices, alerting, watching };
        close(fanout.reporters, closing, (ran, run)).await
    }

    /// The reporters `request` chooses, the reporter plugins among `plugins`, recording into the
    /// history, and a notice for each plugin the run cannot use; a history that cannot be opened
    /// leaves the run unrecorded, with a notice saying so.
    fn reporters(
        &self,
        request: &RunRequest,
        prepared: &Prepared,
        (seed, plugins): (u64, Hosts),
    ) -> Result<(Reporters, Vec<String>), EngineError> {
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
        let mut notices = plugins.notices();
        let recording = self.recording(&start).inspect_err(|error| {
            notices.push(format!("history: {error}; this run is not recorded"));
        });
        let headline = format!("plan  {}", prepared.report().line());
        let (program, plan) = (Arc::clone(&prepared.program), Arc::clone(&prepared.plan));
        let setup = Setup { program, plan, headline, color: request.color, seed, quiet: request.quiet(), plugins };
        Ok((Reporters::create(chosen, setup, recording.ok())?, notices))
    }

    /// The executor for `prepared` with the inputs and plugins `given`, keeping `cache:`d results in the cross-run
    /// cache unless `request` says `--no-cache`, and what the cache leaves for the run's end.
    async fn executor(
        &self,
        prepared: &Prepared,
        given: (RunInputs, &Hosts),
        request: &RunRequest,
    ) -> Result<(Executor, Sweep), EngineError> {
        let (settings, defaults) = (&prepared.settings, &prepared.program.config.defaults);
        let concurrency = settings.concurrency.or(defaults.concurrency).unwrap_or(CONCURRENCY);
        let cx = self.action_cx(prepared, given)?;
        let sweep = if request.no_cache { Sweep::off(cx, Vec::new()) } else { self.cached(cx, prepared).await };
        let options = RunOptions { concurrency: widen(concurrency), bail: request.bail, ..RunOptions::default() };
        Ok((Executor::new(sweep.cx(), options), sweep))
    }

    /// What the actions of `prepared` share with `inputs`: the action executors and the auth
    /// types over one transport, the auth plugins among `plugins` (the run's, built once from
    /// `inputs`' secrets), and events masked by the config's redact rules (a `function:` rule
    /// through the hook runner). Every run, call and cache cleanup starts from it.
    pub(crate) fn action_cx(
        &self,
        prepared: &Prepared,
        (inputs, plugins): (RunInputs, &Hosts),
    ) -> Result<ActionCx, EngineError> {
        let hooks = self.hook_runner(&prepared.program);
        let rules = MaskRules::standard().with_hooks(hooks.clone(), &prepared.program);
        let transport = self.transport(prepared.environment.as_ref())?;
        let auth = self.auth(Arc::clone(&transport), plugins);
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

/// What a run's end needs besides its reporters: the seed, the history run, the notices so
/// far, the alerts and the watch.
struct Closing {
    seed: u64,
    run: Option<u64>,
    notices: Vec<String>,
    alerting: Option<Alerting>,
    watching: Option<Watching>,
}

/// Finishes `reporters` with `run`, then, when the executor `ran` the plan, sends the alerts and
/// gives the report; otherwise gives the executor's error. The watch gets `Done` either way.
async fn close(
    reporters: Reporters,
    closing: Closing,
    (ran, run): (Result<vc_executor::RunSummary, EngineError>, RunSummary),
) -> Result<RunReport, EngineError> {
    let Closing { seed, run: id, mut notices, alerting, watching } = closing;
    let (problems, ending) = reporters.finish(run.clone()).await;
    notices.extend(problems);
    let ran = match ran {
        Ok(ran) => ran,
        Err(error) => {
            notices.push(error.to_string());
            let exit = if error.is_internal() { EXIT_INTERNAL } else { vc_executor::EXIT_NOTHING_RAN };
            if let Some(watching) = watching {
                watching.finish(RunReport { exit, seed, notices, run: id });
            }
            return Err(error);
        }
    };
    notices.extend(notify::send(alerting, run, ending).await);
    let report = RunReport { exit: vc_executor::exit_code(ran), seed, notices, run: id };
    if let Some(watching) = watching {
        watching.finish(report.clone());
    }
    Ok(report)
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

/// Waits for the run `handle` started to finish, handing events to the reporters and interrupts
/// to the run as they come.
async fn pump(
    handle: Result<RunHandle, ExecutorError>,
    fanout: &mut Fanout,
    interrupts: &mut Interrupts,
) -> Result<vc_executor::RunSummary, EngineError> {
    let handle = handle.map_err(|error| executor_error(&error))?;
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
pub fn executor_error(error: &ExecutorError) -> EngineError {
    match error {
        ExecutorError::NoExecutor { .. } => EngineError::Selection {
            reason: error.to_string(),
            help: Some("leave the `function:` action out of the selected tests and lifecycle blocks".to_owned()),
        },
        _ => EngineError::Internal { reason: error.to_string() },
    }
}
