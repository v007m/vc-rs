//! `vc run`: plan, execute, and fan the events out to the reporters.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::Receiver;
use vc_executor::{
    ActionCx, ActionExecutors, Defined, EventBus, Executor, ExecutorError, HostLimits, PipelineParts, RunHandle,
    RunInputs, RunOptions,
};
use vc_interfaces::{ConfigEntry, Random, RunSummary};
use vc_model::{Feed, Part, Template, TemplateValue};

use crate::plan::Prepared;
use crate::reporting::{self, Reporters, Setup};
use crate::{Engine, EngineError, Interrupts, RunRequest, ports};

/// Room for this many events between the bus and the reporters before they miss some.
const FEED: usize = 65_536;
/// Test instances at once, when neither a flag nor the config says.
const CONCURRENCY: u32 = 16;
/// Requests at once to one host, when neither a layer nor the config says.
const PER_HOST: u32 = 8;

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
    /// The errors of [`Engine::plan`]; [`EngineError::Reporter`] for a reporter that is unknown,
    /// given to the wrong flag or not built yet; [`EngineError::Selection`] when the plan
    /// reaches an action this build cannot run.
    pub async fn run(&self, request: &RunRequest, interrupts: Interrupts) -> Result<RunReport, EngineError> {
        let prepared = self.prepare(&request.plan, flags(request))?;
        let chosen = reporting::choose(request.console.as_deref(), &prepared.settings, &prepared.program)?;
        let chosen = chosen.under(self.root());
        let (seed, random) = ports::random(request.seed);
        let executor = self.executor(&prepared, random, request.bail)?;
        let headline = format!("plan  {}", prepared.report().line());
        let setup = Setup {
            program: Arc::clone(&prepared.program),
            plan: Arc::clone(&prepared.plan),
            headline,
            color: request.color,
        };
        let reporters = Reporters::create(&chosen, setup)?;
        let bus = EventBus::spawn();
        let mut fanout = Fanout { feed: bus.subscribe(FEED), reporters };
        let started = self.clock.now_ms();
        let handle = executor.start(Arc::clone(&prepared.plan), bus.clone()).map_err(|e| executor_error(&e))?;
        let summary = pump(handle, &mut fanout, interrupts).await?;
        bus.flush().await;
        while let Ok(next) = fanout.feed.try_recv() {
            fanout.reporters.on(next).await;
        }
        let elapsed = Duration::from_millis(self.clock.now_ms().saturating_sub(started));
        let run =
            RunSummary::new(summary.status, summary.totals).timed(started, elapsed).profiles(prepared.names.clone());
        let mut notices = prepared.notices;
        notices.extend(fanout.reporters.finish(run).await);
        Ok(RunReport { exit: vc_executor::exit_code(summary), seed, notices })
    }

    /// The executor for `prepared`: HTTP through the per-host limits, the active profiles and
    /// the `--set vars.*` values.
    fn executor(&self, prepared: &Prepared, random: Arc<dyn Random>, bail: bool) -> Result<Executor, EngineError> {
        let (settings, defaults) = (&prepared.settings, &prepared.program.config.defaults);
        let per_host = settings.per_host.or(defaults.per_host).unwrap_or(PER_HOST);
        let concurrency = settings.concurrency.or(defaults.concurrency).unwrap_or(CONCURRENCY);
        let transport = self.transport(prepared.environment.as_ref())?;
        let parts = PipelineParts::new(transport, HostLimits::spawn(widen(per_host)));
        let inputs = RunInputs::new(random, Arc::clone(&prepared.environment))
            .with_profiles(prepared.profiles.clone())
            .with_vars(vars(&settings.vars));
        let cx = ActionCx::new(Arc::clone(&prepared.program), Arc::new(ActionExecutors::standard(parts)), inputs);
        let options = RunOptions { concurrency: widen(concurrency), bail, ..RunOptions::default() };
        Ok(Executor::new(Arc::new(cx), options))
    }
}

/// The run flags that are settings, as the layer between `VIB_*` and `--set` (D129).
fn flags(request: &RunRequest) -> Vec<ConfigEntry> {
    let mut flags = Vec::new();
    if let Some(concurrency) = request.concurrency {
        flags.push(ConfigEntry::new("defaults.concurrency", concurrency.to_string(), "--concurrency"));
    }
    if !request.reports.is_empty() {
        flags.push(ConfigEntry::new("reports.formats", request.reports.join(","), "--report"));
    }
    if let Some(out) = &request.out {
        flags.push(ConfigEntry::new("reports.dir", out.to_string_lossy(), "--out"));
    }
    flags
}

/// The run's event feed and the reporters it fans out to.
struct Fanout {
    feed: Receiver<Feed>,
    reporters: Reporters,
}

/// Waits for the run to finish, handing events to the reporters and interrupts to the run as
/// they come.
async fn pump(
    handle: RunHandle,
    fanout: &mut Fanout,
    mut interrupts: Interrupts,
) -> Result<vc_executor::RunSummary, EngineError> {
    let interrupter = handle.interrupter();
    let finish = handle.finish();
    tokio::pin!(finish);
    loop {
        tokio::select! {
            biased;
            summary = &mut finish => return summary.map_err(|e| executor_error(&e)),
            Some(()) = interrupts.inbox.recv() => interrupter.interrupt(),
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
            help: Some("leave the `command:` action out of the selected tests and lifecycle blocks".to_owned()),
        },
        _ => EngineError::Internal { reason: error.to_string() },
    }
}
