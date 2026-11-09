//! The run's reporters (D43): one console format, fed in run order from the event bus, and the
//! report files, rendered from the run's snapshot once it ends (D167).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vc_adapter_reporters::{
    Color, Format, FormatRegistry, FsDestination, HtmlLayout, Recorder, ReporterContext, ReporterRegistry, RunFacts,
    RunSnapshot,
};
use vc_interfaces::{Artifact, ReportError, Reporter, RunEnd, RunId, RunSummary};
use vc_model::{Feed, Keep, Plan, Program};

use crate::EngineError;
use crate::history::Recording;
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
    ("html", Role::File),
];

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
}

impl Chosen {
    /// The same choice with a relative report folder under `root`.
    #[must_use]
    pub fn under(self, root: &Path) -> Self {
        Self { dir: root.join(self.dir), ..self }
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
        Ok(render(&self.formats()?, snapshot, &self.dir_for(Some(run))))
    }
}

/// Picks the console format (`--reporter`, else `spec`), the report files (`--report`, a layer,
/// else the config's `reports.formats`, else `ctrf`), their folder (`--out`, a layer, else
/// the config's `reports.dir`, else `.vibranium/reports`), which may be relative, and the HTML
/// layout (`--single-file`, a layer, else the config's `reports.html.layout`).
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
    for file in &files {
        check(file, &named.origin, Role::File)?;
    }
    let dir = settings.reports_dir.as_deref().or(program.config.reports_dir.as_deref()).unwrap_or(REPORTS_DIR);
    let single_file = settings.html_single_file.unwrap_or(program.config.html_single_file);
    Ok(Chosen { console: console.to_owned(), files, dir: PathBuf::from(dir), per_run: true, single_file })
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
}

impl Reporters {
    /// The reporters and formats `chosen` names, built for `setup`, recording into `history`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Internal`] when a checked name is not registered.
    pub fn create(chosen: Chosen, setup: Setup, history: Option<Recording>) -> Result<Self, EngineError> {
        let destination = Arc::new(FsDestination::new(&chosen.dir));
        let color = if setup.color { Color::Ansi } else { Color::Plain };
        let keep = setup.program.config.history.keep;
        let context =
            ReporterContext::new(setup.program, setup.plan, destination).color(color).headline(setup.headline);
        let internal = |error: ReportError| EngineError::Internal { reason: error.to_string() };
        let console = ReporterRegistry::builtin().create(&chosen.console, &context).map_err(internal)?;
        let formats = chosen.formats()?;
        let recorder = Recorder::new(&context);
        let problems = BTreeSet::new();
        Ok(Self { console, recorder, formats, chosen, history, keep, seed: setup.seed, problems })
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
        let rows = self.recorder.take(&event);
        if let Some(Err(error)) = self.history.as_ref().map(|history| history.record(rows)) {
            self.problems.insert(format!("history: {error}"));
        }
    }

    /// Writes the report files and closes the run in the history, then ends the console with
    /// the files' paths; the problems, one line each.
    pub async fn finish(self, run: RunSummary) -> Vec<String> {
        let Self { mut console, recorder, formats, chosen, history, keep, seed, mut problems } = self;
        let id = history.as_ref().map_or(0, |history| history.id().get());
        let snapshot = recorder.finish(RunFacts { seed: Some(seed), ..RunFacts::of(id, &run) });
        let dir = chosen.dir_for(history.as_ref().map(Recording::id));
        let (artifacts, failed) = render(&formats, &snapshot, &dir);
        problems.extend(failed);
        if let Some(history) = history
            && let Err(error) = close(history, keep, (&run, &snapshot)).await
        {
            problems.insert(format!("history: {error}"));
        }
        if let Err(error) = console.finish(&run.artifacts(artifacts)).await {
            problems.insert(error.to_string());
        }
        problems.into_iter().collect()
    }
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
