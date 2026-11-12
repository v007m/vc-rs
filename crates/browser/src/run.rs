//! `vc run` in the playground: the plan of `prepare`, run by the real executor against the demo
//! service in the page, with the spec reporter printing to the page's console. The wiring
//! mirrors the engine's `run.rs`, less what a page cannot hold: the history, the cross-run
//! cache, report files, plugins, hooks, programs (see `commands.rs`), secrets and alerts.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::Receiver;
use vc_adapter_auth::AuthTypes;
use vc_adapter_reporters::{Color, ReporterContext, ReporterRegistry};
use vc_adapter_system::SystemClock;
use vc_executor::{
    ActionCx, ActionExecutors, ActionKindTag, CommandExecutor, CommandShell, Defined, EventBus, Executor,
    ExecutorError, HostLimits, MaskRules, PipelineParts, RunHandle, RunInputs, RunOptions,
};
use vc_interfaces::{Clock as _, MemorySources, Reporter, RunSummary, Secrets, Transport};
use vc_model::{Feed, RunStatus, Totals};

use crate::commands::PageCommands;
use crate::console::SinkDestination;
use crate::prepare::{self, Prepared};
use crate::{BrowserError, InProcessTransport, Sink, bodies, compile, environment, unsupported};

/// Room for this many events between the bus and the console before it misses some.
const FEED: usize = 65_536;
/// Test instances at once, when the config does not say.
const CONCURRENCY: u32 = 16;
/// Requests at once to one host, when the config does not say.
const PER_HOST: u32 = 8;

/// How a playground run ended: its exit code, its seed and its notices.
#[derive(Debug)]
pub struct Ran {
    pub exit: u8,
    pub seed: u64,
    pub notices: Vec<String>,
}

/// Compiles, plans and runs `project`, printing the report to `sink`, `color`ed or not,
/// drawing from `seed` or a fresh one.
pub async fn run(
    project: &MemorySources,
    sink: Arc<dyn Sink>,
    (color, seed): (bool, Option<u64>),
) -> Result<Ran, BrowserError> {
    let today = SystemClock.today();
    let program = compile::compile(project, today)?.valid()?;
    unsupported::refuse(&program)?;
    let prepared = prepare::prepare(program, environment::page(), (seed, today.to_string()))?;
    let mut notices = BTreeSet::from_iter(prepared.notices.iter().cloned());
    notices.extend(unsupported::notices(&prepared.program));
    let executor = executor(&prepared, project, Arc::new(InProcessTransport::demo()));
    let mut console = console(&prepared, sink, color)?;
    let (summary, started, elapsed) = execute(&executor, &prepared, (&mut *console, &mut notices)).await;
    let (status, totals) =
        summary.as_ref().map_or_else(|_| (RunStatus::Cancelled, Totals::default()), |ran| (ran.status, ran.totals));
    let run = RunSummary::new(status, totals).timed(started, elapsed).profiles(prepared.names.clone());
    if let Err(error) = console.finish(&run).await {
        notices.insert(error.to_string());
    }
    let exit = vc_executor::exit_code(summary?);
    Ok(Ran { exit, seed: prepared.seed, notices: notices.into_iter().collect() })
}

/// Starts the run and pumps its events to `console` until it ends: what it summed up, its
/// start and how long it took.
async fn execute(
    executor: &Executor,
    prepared: &Prepared,
    (console, notices): (&mut dyn Reporter, &mut BTreeSet<String>),
) -> (Result<vc_executor::RunSummary, BrowserError>, u64, Duration) {
    let bus = EventBus::spawn();
    let mut feed = bus.subscribe(FEED);
    let started = SystemClock.now_ms();
    let summary = pump(executor.start(Arc::clone(&prepared.plan), bus.clone()), &mut feed, (console, notices)).await;
    bus.flush().await;
    while let Ok(next) = feed.try_recv() {
        on(next, console, notices).await;
    }
    (summary, started, Duration::from_millis(SystemClock.now_ms().saturating_sub(started)))
}

/// Waits for the run `handle` started to finish, handing its events to `console` as they come.
async fn pump(
    handle: Result<RunHandle, ExecutorError>,
    feed: &mut Receiver<Feed>,
    (console, notices): (&mut dyn Reporter, &mut BTreeSet<String>),
) -> Result<vc_executor::RunSummary, BrowserError> {
    let finish = handle.map_err(|error| executor_error(&error))?.finish();
    tokio::pin!(finish);
    loop {
        tokio::select! {
            biased;
            summary = &mut finish => return summary.map_err(|error| executor_error(&error)),
            Some(next) = feed.recv() => on(next, console, notices).await,
        }
    }
}

/// Hands one event to `console`; a missed stretch, or an event it could not print, is a notice.
async fn on(next: Feed, console: &mut dyn Reporter, notices: &mut BTreeSet<String>) {
    let event = match next {
        Feed::Event(sequenced) => sequenced.event,
        Feed::Missed { count } => {
            notices.insert(format!("the reporters missed {count} events; the reports are incomplete"));
            return;
        }
    };
    if let Err(error) = console.on_event(&event).await {
        notices.insert(error.to_string());
    }
}

/// The spec reporter, printing to `sink` under the plan line `vc run` heads its report with.
fn console(prepared: &Prepared, sink: Arc<dyn Sink>, color: bool) -> Result<Box<dyn Reporter>, BrowserError> {
    let destination = Arc::new(SinkDestination::new(sink));
    let color = if color { Color::Ansi } else { Color::Plain };
    let context = ReporterContext::new(Arc::clone(&prepared.program), Arc::clone(&prepared.plan), destination)
        .color(color)
        .headline(prepare::headline(prepared));
    ReporterRegistry::builtin().create("spec", &context).map_err(crate::print::internal)
}

/// The executor of `prepared`: HTTP through `transport` under the config's limits, `command:`
/// lines through [`PageCommands`] in the `sh -c` form whatever the host, the auth types
/// a page can run, events masked by the standard and the config's redact rules, and no secrets.
fn executor(prepared: &Prepared, project: &MemorySources, transport: Arc<dyn Transport>) -> Executor {
    let defaults = &prepared.program.config.defaults;
    let per_host = widen(defaults.per_host.unwrap_or(PER_HOST));
    let parts = PipelineParts::new(Arc::clone(&transport), HostLimits::spawn(per_host));
    let commands =
        CommandExecutor::new(Arc::new(PageCommands), PathBuf::new(), Vec::new()).with_shell(CommandShell::POSIX);
    let executors = Arc::new(ActionExecutors::standard(parts).with(ActionKindTag::Command, Arc::new(commands)));
    let cx = ActionCx::new(Arc::clone(&prepared.program), executors, inputs(prepared, project))
        .with_mask_rules(&MaskRules::standard())
        .with_auth(Arc::new(AuthTypes::headless(transport)));
    let concurrency = widen(defaults.concurrency.unwrap_or(CONCURRENCY));
    Executor::new(Arc::new(cx), RunOptions { concurrency, ..RunOptions::default() })
}

/// What the run's templates read besides the definitions: the seeded random values, the
/// page's empty environment, the default profiles, the drawn rows and the `$file` bodies.
fn inputs(prepared: &Prepared, project: &MemorySources) -> RunInputs {
    let bodies = bodies::read(project, &prepared.program, &prepared.plan);
    RunInputs::new(Arc::clone(&prepared.random), environment::page())
        .with_profiles(prepared.profiles.clone())
        .with_vars(Defined::new())
        .with_run_id(0)
        .with_bodies(bodies)
        .with_examples(prepared.rows.clone())
        .with_secrets(Secrets::new())
        .with_started_at(SystemClock.now_ms())
}

fn widen(count: u32) -> usize {
    usize::try_from(count).unwrap_or(usize::MAX)
}

/// An action the playground cannot run is the user's to avoid; anything else is a bug. Hooks
/// never get here: [`unsupported::refuse`] refuses them before the run.
fn executor_error(error: &ExecutorError) -> BrowserError {
    match error {
        ExecutorError::NoExecutor { .. } => {
            let help = "leave that action out of the selected tests and lifecycle blocks, or run the project with \
                        `vc run` on your machine";
            BrowserError::refused(error.to_string(), Some(help.to_owned()))
        }
        _ => BrowserError::Internal { reason: error.to_string() },
    }
}
