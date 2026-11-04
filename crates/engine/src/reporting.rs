//! The run's reporters (D43): one console format and the report files, fed in run order from
//! the event bus.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vc_adapter_reporters::{Color, FsDestination, ReporterContext, ReporterRegistry};
use vc_interfaces::{Artifact, Reporter, RunSummary};
use vc_model::{Feed, Plan, Program};

use crate::EngineError;
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
    /// A report file that is planned but not built yet.
    LaterFile,
}

/// Every reporter name the CLI documents, and its role.
const ROLES: &[(&str, Role)] = &[
    ("spec", Role::Console),
    ("json", Role::Console),
    ("dots", Role::LaterConsole),
    ("ctrf", Role::File),
    ("junit", Role::File),
    ("html", Role::LaterFile),
];

/// The console format when none is given.
const CONSOLE: &str = "spec";
/// The report files when neither a flag, a layer nor the config names any.
const FILES: &[&str] = &["ctrf"];
/// Where reports go when nothing says, under the project root.
const REPORTS_DIR: &str = ".vibranium/reports";
/// The origin of values from the project's config.
const CONFIG: &str = "vibranium.config.yaml";

/// The reporters a run uses and where its report files go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chosen {
    pub console: String,
    pub files: Vec<String>,
    pub dir: PathBuf,
}

impl Chosen {
    /// The same choice with a relative report folder under `root`.
    #[must_use]
    pub fn under(self, root: &Path) -> Self {
        Self { dir: root.join(self.dir), ..self }
    }
}

/// Picks the console format (`--reporter`, else `spec`), the report files (`--report`, a layer,
/// else the config's `reports.formats`, else `ctrf`) and their folder (`--out`, a layer, else
/// the config's `reports.dir`, else `.vibranium/reports`), which may be relative.
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
    Ok(Chosen { console: console.to_owned(), files, dir: PathBuf::from(dir) })
}

/// Whether `name`, given by `origin`, is a reporter of role `wanted`.
fn check(name: &str, origin: &str, wanted: Role) -> Result<(), EngineError> {
    let role = ROLES.iter().find(|(known, _)| *known == name).map(|(_, role)| *role);
    let error = |reason: String, help: Option<String>| {
        Err(EngineError::Reporter { reason: format!("{origin}: {reason}"), help })
    };
    match (wanted, role) {
        (_, Some(role)) if role == wanted => Ok(()),
        (_, Some(Role::LaterConsole | Role::LaterFile)) => error(format!("`{name}` is not supported yet"), None),
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
}

/// The run's reporters, and the problems they had; a report failure never changes a result.
pub struct Reporters {
    console: Box<dyn Reporter>,
    files: Vec<Box<dyn Reporter>>,
    problems: BTreeSet<String>,
}

impl Reporters {
    /// The reporters `chosen` names, built for `setup`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Internal`] when a checked name is not registered.
    pub fn create(chosen: &Chosen, setup: Setup) -> Result<Self, EngineError> {
        let destination = Arc::new(FsDestination::new(&chosen.dir));
        let color = if setup.color { Color::Ansi } else { Color::Plain };
        let context =
            ReporterContext::new(setup.program, setup.plan, destination).color(color).headline(setup.headline);
        let registry = ReporterRegistry::builtin();
        let create =
            |name: &str| registry.create(name, &context).map_err(|e| EngineError::Internal { reason: e.to_string() });
        let files = chosen.files.iter().map(|name| create(name)).collect::<Result<_, _>>()?;
        Ok(Self { console: create(&chosen.console)?, files, problems: BTreeSet::new() })
    }

    /// Hands the next event to every reporter.
    pub async fn on(&mut self, feed: Feed) {
        let event = match feed {
            Feed::Event(sequenced) => sequenced.event,
            Feed::Missed { count } => {
                self.problems.insert(format!("the reporters missed {count} events; the reports are incomplete"));
                return;
            }
        };
        for reporter in self.files.iter_mut().chain([&mut self.console]) {
            if let Err(error) = reporter.on_event(&event).await {
                self.problems.insert(error.to_string());
            }
        }
    }

    /// Ends the report files, then the console with their paths; the problems, one line each.
    pub async fn finish(mut self, run: RunSummary) -> Vec<String> {
        let mut artifacts: Vec<Artifact> = Vec::new();
        for reporter in &mut self.files {
            match reporter.finish(&run).await {
                Ok(written) => artifacts.extend(written),
                Err(error) => {
                    self.problems.insert(error.to_string());
                }
            }
        }
        if let Err(error) = self.console.finish(&run.artifacts(artifacts)).await {
            self.problems.insert(error.to_string());
        }
        self.problems.into_iter().collect()
    }
}

#[cfg(test)]
#[path = "reporting_tests.rs"]
mod tests;
