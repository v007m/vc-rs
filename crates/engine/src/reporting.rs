//! The run's reporters (D43): one console format, fed in run order from the event bus, the
//! report files, rendered from the run's snapshot once it ends (D167), and the reporter plugins
//! `reports.formats` names (G6), fed the same events.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vc_adapter_reporters::{
    Color, Format, FormatRegistry, FsDestination, HtmlLayout, PluginReporter, Recorder, ReporterContext,
    ReporterRegistry, RunFacts, RunSnapshot, mark_run_folder,
};
use vc_diagnostics::Code;
use vc_interfaces::{Artifact, PluginKind, ReportError, Reporter, RunEnd, RunId, RunSummary};
use vc_model::{Event, Feed, Keep, Plan, Program};

use crate::EngineError;
use crate::alerts::{Ending, Watch};
use crate::history::Recording;
use crate::plugin_hosts::Hosts;
use crate::settings::{Named, Settings};

/// What a reporter name is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// A `--reporter` console format.
    Console,
    /// A `--report` file.
    File,
    /// A console format that is planned but not built yet.
    LaterConsole,
}

/// Every reporter name the CLI documents, and its role.
const ROLES: &[(&str, Role)] = &[
    ("spec", Role::Console),
    ("json", Role::Console),
    ("dots", Role::LaterConsole),
    ("ctrf", Role::File),
    ("junit", Role::File),
    (HTML, Role::File),
];

/// The HTML report's name, whose file the live run names (D320).
pub const HTML: &str = "html";
/// The console format when none is given.
const CONSOLE: &str = "spec";
/// The report files when neither a flag, a layer nor the config names any.
pub const FILES: &[&str] = &["ctrf"];
/// Where reports go when nothing says, under the project root.
pub const REPORTS_DIR: &str = ".vibranium/reports";
/// The origin of values from the project's config.
const CONFIG: &str = "vibranium.config.yaml";

/// The report formats a run or `vc report` writes.
type Formats = Vec<Arc<dyn Format>>;

/// The files a render wrote, and why any were not.
type Rendered = (Vec<Artifact>, Vec<String>);

/// The reporters a run uses and where its report files go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chosen {
    pub console: String,
    pub files: Vec<String>,
    pub dir: PathBuf,
    /// Whether each run's files go in a folder of their own, named by its history number.
    pub per_run: bool,
    /// Whether the HTML report is one self-contained file.
    pub single_file: bool,
    /// The reporter plugins it names, by ID.
    pub plugins: Vec<String>,
}

impl Chosen {
    /// The same choice with a relative report folder under `root`.
    #[must_use]
    pub fn under(self, root: &Path) -> Self {
        Self { dir: crate::paths::under(root, self.dir), ..self }
    }

    /// The same choice writing straight into its folder when `flat`, as `--out` does.
    #[must_use]
    pub fn flat(self, flat: bool) -> Self {
        Self { per_run: self.per_run && !flat, ..self }
    }

    /// Where the files of history run `run` go: `<dir>/<run>/`, or `<dir>` itself when the
    /// files go straight in or the run is not in the history.
    #[must_use]
    pub fn dir_for(&self, run: Option<RunId>) -> PathBuf {
        match run {
            Some(run) if self.per_run => self.dir.join(run.to_string()),
            _ => self.dir.clone(),
        }
    }

    /// The formats it names, the HTML report laid out as it says.
    ///
    /// # Errors
    ///
    /// [`EngineError::Internal`] when a checked name is not registered.
    fn formats(&self) -> Result<Formats, EngineError> {
        let layout = if self.single_file { HtmlLayout::Single } else { HtmlLayout::Folder };
        let registry = FormatRegistry::builtin(layout);
        let get =
            |name: &String| registry.get(name).map_err(|error| EngineError::Internal { reason: error.to_string() });
        self.files.iter().map(get).collect()
    }

    /// Renders `snapshot` of history run `run` in every format it names, into
    /// [`Chosen::dir_for`]: the files written, and why any were not.
    ///
    /// # Errors
    ///
    /// [`EngineError::Internal`] when a checked name is not registered.
    pub(crate) fn render(&self, snapshot: &RunSnapshot, run: RunId) -> Result<Rendered, EngineError> {
        let formats = self.formats()?;
        let (dir, unmarked) = self.run_folder(Some(run));
        let (artifacts, mut problems) = render(&formats, snapshot, &dir);
        problems.extend(unmarked);
        Ok((artifacts, problems))
    }

    /// [`Chosen::dir_for`] `run`, made and marked as a run's folder ([`mark_run_folder`]) when
    /// the run's files go in a folder of their own, so pruning never takes any other: the
    /// folder, and why it could not be marked.
    fn run_folder(&self, run: Option<RunId>) -> (PathBuf, Option<String>) {
        let dir = self.dir_for(run);
        let unmarked = (self.per_run && run.is_some()).then(|| mark_run_folder(&dir)).and_then(Result::err);
        (dir, unmarked.map(|error| error.to_string()))
    }
}

/// Picks the console format (`--reporter`, else `spec`), the report files (`--report`, a layer,
/// else the config's `reports.formats`, else `ctrf`) and the reporter plugins among them, their
/// folder (`--out`, a layer, else the config's `reports.dir`, else `.vibranium/reports`), which
/// may be relative, and the HTML layout (`--single-file`, a layer, else the config's
/// `reports.html.layout`).
///
/// # Errors
///
/// [`EngineError::Reporter`] for an unknown name, one given to the wrong flag, or one that is
/// not built yet.
pub fn choose(console: Option<&str>, settings: &Settings, program: &Program) -> Result<Chosen, EngineError> {
    let console = console.unwrap_or(CONSOLE);
    check(console, "--reporter", Role::Console)?;
    let config = Named { names: program.config.report_formats.clone(), origin: CONFIG.to_owned() };
    let named = settings.formats.clone().unwrap_or(config);
    let files =
        if named.names.is_empty() { FILES.iter().map(|name| (*name).to_owned()).collect() } else { named.names };
    let configured = |name: &String| program.config.plugins.iter().any(|plugin| program.str(plugin.id) == Some(name));
    let (plugins, files): (Vec<String>, Vec<String>) = files.into_iter().partition(configured);
    for file in &files {
        check(file, &named.origin, Role::File)?;
    }
    let dir = settings.reports_dir.as_deref().or(program.config.reports_dir.as_deref()).unwrap_or(REPORTS_DIR);
    let single_file = settings.html_single_file.unwrap_or(program.config.html_single_file);
    Ok(Chosen { console: console.to_owned(), files, dir: PathBuf::from(dir), per_run: true, single_file, plugins })
}

/// Whether `name`, given by `origin`, is a reporter of role `wanted`.
fn check(name: &str, origin: &str, wanted: Role) -> Result<(), EngineError> {
    let role = ROLES.iter().find(|(known, _)| *known == name).map(|(_, role)| *role);
    let error = |reason: String, help: Option<String>| {
        Err(EngineError::Reporter { reason: format!("{origin}: {reason}"), help })
    };
    match (wanted, role) {
        (_, Some(role)) if role == wanted => Ok(()),
        (_, Some(Role::LaterConsole)) => error(format!("`{name}` is not supported yet"), None),
        (Role::Console, Some(_)) => {
            error(format!("`{name}` is a report file"), Some(format!("pass it to `--report {name}`")))
        }
        (_, Some(_)) => error(format!("`{name}` is a console format"), Some(format!("pass it to `--reporter {name}`"))),
        (Role::Console, None) => error(format!("no console format `{name}`"), Some(known(Role::Console))),
        (_, None) => error(format!("no report format `{name}`"), Some(known(Role::File))),
    }
}

fn known(role: Role) -> String {
    let names: Vec<&str> = ROLES.iter().filter(|(_, r)| *r == role).map(|(name, _)| *name).collect();
    format!("the {} are {}", if role == Role::Console { "console formats" } else { "report formats" }, names.join(", "))
}

/// What the reporters are built from, besides the names.
pub struct Setup {
    pub program: Arc<Program>,
    pub plan: Arc<Plan>,
    pub headline: String,
    pub color: bool,
    pub seed: u64,
    /// Whether the console prints nothing, for a run an agent started over stdio.
    pub quiet: bool,
    /// The run's plugins, among them the reporter plugins the choice names.
    pub plugins: Hosts,
    /// The project root, which the HTML report's path is named from.
    pub root: PathBuf,
}

/// How the reporters finished: their problems, one line each, what the alerts read, and where
/// the HTML report was written, relative to the project root, when it was written inside it.
pub struct Reported {
    pub problems: Vec<String>,
    pub ending: Ending,
    pub html: Option<PathBuf>,
}

/// The run's reporters, its record and its report files, and the problems they had; a report
/// or history failure never changes a result.
pub struct Reporters {
    console: Box<dyn Reporter>,
    recorder: Recorder,
    formats: Formats,
    chosen: Chosen,
    history: Option<Recording>,
    keep: Keep,
    seed: u64,
    problems: BTreeSet<String>,
    /// What the `alerts` rules read, when there are any.
    watch: Option<Watch>,
    /// The reporter plugins still taking events.
    plugins: Vec<PluginReporter>,
    root: PathBuf,
}

impl Reporters {
    /// The reporters and formats `chosen` names, built for `setup`, recording into `history`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Internal`] when a checked name is not registered.
    pub fn create(chosen: Chosen, setup: Setup, history: Option<Recording>) -> Result<Self, EngineError> {
        let (plugins, problems) = plugins(&chosen, &setup, history.as_ref().map(Recording::id));
        let destination = Arc::new(FsDestination::new(&chosen.dir).quiet(setup.quiet));
        let color = if setup.color { Color::Ansi } else { Color::Plain };
        let keep = setup.program.config.history.keep;
        let watch = (!setup.program.config.alerts.is_empty()).then(|| Watch::of(&setup.program));
        let context =
            ReporterContext::new(setup.program, setup.plan, destination).color(color).headline(setup.headline);
        let internal = |error: ReportError| EngineError::Internal { reason: error.to_string() };
        let console = ReporterRegistry::builtin().create(&chosen.console, &context).map_err(internal)?;
        let formats = chosen.formats()?;
        let recorder = Recorder::new(&context);
        let (seed, root) = (setup.seed, setup.root);
        Ok(Self { console, recorder, formats, chosen, history, keep, seed, problems, watch, plugins, root })
    }

    /// The run's number in the history, when it is recorded.
    pub fn run_id(&self) -> Option<RunId> {
        self.history.as_ref().map(Recording::id)
    }

    /// Hands the next event to the console and the record, and the rows it completes to the
    /// history.
    pub async fn on(&mut self, feed: Feed) {
        let event = match feed {
            Feed::Event(sequenced) => sequenced.event,
            Feed::Missed { count } => {
                self.problems.insert(format!("the reporters missed {count} events; the reports are incomplete"));
                return;
            }
        };
        if let Err(error) = self.console.on_event(&event).await {
            self.problems.insert(error.to_string());
        }
        self.feed(&event).await;
        let rows = self.recorder.take(&event);
        if let Some(watch) = self.watch.as_mut() {
            watch.see(&rows);
        }
        if let Some(Err(error)) = self.history.as_ref().map(|history| history.record(rows)) {
            self.problems.insert(format!("history: {error}"));
        }
    }

    /// Hands `event` to each reporter plugin; one that fails is dropped, and named once.
    async fn feed(&mut self, event: &Event) {
        let mut kept = Vec::with_capacity(self.plugins.len());
        for mut plugin in std::mem::take(&mut self.plugins) {
            match plugin.on_event(event).await {
                Ok(()) => kept.push(plugin),
                Err(error) => {
                    self.problems.insert(format!("{}: {error}; its report is not written", Code::V5118));
                }
            }
        }
        self.plugins = kept;
    }

    /// Writes the report files and closes the run in the history, then ends the console with
    /// the files' paths.
    pub async fn finish(self, run: RunSummary) -> Reported {
        let Self { mut console, recorder, formats, chosen, history, keep, seed, mut problems, watch, plugins, root } =
            self;
        let id = history.as_ref().map_or(0, |history| history.id().get());
        let snapshot = recorder.finish(RunFacts { seed: Some(seed), ..RunFacts::of(id, &run) });
        let (dir, unmarked) = chosen.run_folder(history.as_ref().map(Recording::id));
        problems.extend(unmarked);
        let (mut artifacts, failed) = render(&formats, &snapshot, &dir);
        problems.extend(failed);
        let (written, failed) = finish(plugins, &run).await;
        artifacts.extend(written);
        problems.extend(failed);
        let report = artifacts.first().map(|artifact| artifact.path.clone());
        let html =
            artifacts.iter().find(|artifact| artifact.reporter == HTML).and_then(|html| inside(&root, &html.path));
        let ending = watch.map(|watch| watch.ending(report)).unwrap_or_default();
        let ending = Ending { teardown: snapshot.teardown_failed(), ..ending };
        if let Some(history) = history
            && let Err(error) = close(history, keep, (&run, &snapshot)).await
        {
            problems.insert(format!("history: {error}"));
        }
        if let Err(error) = console.finish(&run.artifacts(artifacts)).await {
            problems.insert(error.to_string());
        }
        Reported { problems: problems.into_iter().collect(), ending, html }
    }
}

/// The reporter plugins `chosen` names, writing into its folder for history run `run`, and why
/// any cannot report.
fn plugins(chosen: &Chosen, setup: &Setup, run: Option<RunId>) -> (Vec<PluginReporter>, BTreeSet<String>) {
    let destination = Arc::new(FsDestination::new(chosen.dir_for(run)));
    let context = ReporterContext::new(Arc::clone(&setup.program), Arc::clone(&setup.plan), destination);
    let (mut reporters, mut problems) = (Vec::new(), BTreeSet::new());
    for id in &chosen.plugins {
        match setup.plugins.get(id, PluginKind::Reporter) {
            Ok(host) => reporters.push(PluginReporter::new(host, &context)),
            Err(reason) => {
                problems.insert(format!("{reason}; its report is not written"));
            }
        }
    }
    (reporters, problems)
}

/// `path` relative to the project `root`, rebuilt from its parts as [`crate::paths::under`]
/// does; none outside it, so a watcher is never told where the project is, nor a path beyond it.
fn inside(root: &Path, path: &Path) -> Option<PathBuf> {
    path.strip_prefix(root).ok().map(|relative| relative.components().collect())
}

/// Ends each reporter plugin with `run`: the files they wrote, and why any did not.
async fn finish(plugins: Vec<PluginReporter>, run: &RunSummary) -> Rendered {
    let (mut artifacts, mut problems) = (Vec::new(), Vec::new());
    for mut plugin in plugins {
        match plugin.finish(run).await {
            Ok(written) => artifacts.extend(written),
            Err(error) => problems.push(format!("{}: {error}", Code::V5118)),
        }
    }
    (artifacts, problems)
}

/// Renders `snapshot` in every format into `dir`: the files written, and why any were not.
fn render(formats: &[Arc<dyn Format>], snapshot: &RunSnapshot, dir: &Path) -> Rendered {
    let destination = FsDestination::new(dir);
    let (mut artifacts, mut problems) = (Vec::new(), Vec::new());
    for format in formats {
        match format.render(snapshot, &destination) {
            Ok(written) => artifacts.extend(written),
            Err(error) => problems.push(error.to_string()),
        }
    }
    (artifacts, problems)
}

/// Closes `history`'s run as `run` ended, with `snapshot` for `vc report`, then prunes to `keep`.
async fn close(history: Recording, keep: Keep, (run, snapshot): (&RunSummary, &RunSnapshot)) -> Result<(), String> {
    let snapshot = snapshot.to_json().map_err(|error| error.to_string())?;
    let finished_ms = run.started_ms.saturating_add(u64::try_from(run.duration.as_millis()).unwrap_or(u64::MAX));
    let end = RunEnd { status: run.status, totals: run.totals, finished_ms, snapshot };
    history.close(end, keep).await.map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "reporting_tests.rs"]
mod tests;
