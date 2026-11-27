//! `vc init`: a new project that checks clean and runs green offline (D89, D175): the config
//! with a `default` profile, the sample api and test files against the built-in demo service,
//! `.env.schema`, and `.vibranium/` and `.env` in `.gitignore` (D263, D286). It never overwrites a definition file.

use vc_compiler::schema_line;
use vc_model::{DEFAULT_SECRETS_SCHEMA, FileKind, SourcePath};
use vc_parser::write_text;

use crate::authoring::refused;
use crate::edit::{Changes, FileChange, internal};
use crate::import_write::new_file;
use crate::project::sources_error;
use crate::{Engine, EngineError};

/// The config, with `{project}` for the project's name.
const CONFIG: &str = include_str!("init/vibranium.config.yaml");
const SAMPLE_API: &str = include_str!("init/sample.api.yaml");
const SAMPLE_TESTS: &str = include_str!("init/basics.test.yaml");
pub const ENV_SCHEMA: &str = include_str!("init/env.schema");
/// The config's var for the sample's demo service.
const SAMPLE_VAR: &str = "  sampleUrl: http://localhost:7421    # the demo service used by services/sample\n";
/// The config's commented profile var: with the sample, and without it.
const PROFILE_VAR: [&str; 2] = ["sampleUrl: https://sample.staging.example", "baseUrl: https://api.staging.example"];
const GITIGNORE: &str = ".gitignore";
/// The lines `vc init` puts in `.gitignore`: its generated data, and the secrets (D286).
const IGNORED: [&str; 2] = [".vibranium/", ".env"];

/// The definition files `vc init` writes: path, kind and text after the schema line.
const DEFINITIONS: [(&str, FileKind, &str); 3] = [
    ("vibranium.config.yaml", FileKind::Config, CONFIG),
    ("services/sample/sample.api.yaml", FileKind::Api, SAMPLE_API),
    ("services/sample/basics.test.yaml", FileKind::Test, SAMPLE_TESTS),
];

/// The wizard's choices for a new project (D716): each is checked before it gets here.
#[derive(Clone, Debug, Default)]
pub struct Start<'a> {
    /// With the sample api and test files.
    pub sample: bool,
    /// The project's name; the folder's when `None`.
    pub name: Option<&'a str>,
    /// The default profile's ID; `default` when `None`.
    pub profile: Option<&'a str>,
    /// The first service, as a file ID word, given its own api file.
    pub service: Option<&'a str>,
    /// The `baseUrl` var, which the first service's api file uses.
    pub base_url: Option<&'a str>,
}

/// The empty folders of the conventional layout, which the app creates.
pub const INIT_FOLDERS: [&str; 5] = ["schemas", "datasets", "hooks", "openapi", "load"];

impl Engine {
    /// The files of a new project in the engine's root: the config, and with `sample` the
    /// sample api and test files, passed by the compile guard; then `.env.schema` when there is
    /// none, and `.gitignore` with `.vibranium/` and `.env` added when it lacks them.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when a definition file it would write exists (a project is here
    /// already) or the guard refuses; [`EngineError::Sources`] when the folder cannot be read.
    pub fn init(&self, sample: bool) -> Result<Changes, EngineError> {
        self.init_from(&Start { sample, ..Start::default() })
    }

    /// [`Engine::init`] with the wizard's choices in `start`.
    ///
    /// # Errors
    ///
    /// As [`Engine::init`].
    pub(crate) fn init_from(&self, start: &Start<'_>) -> Result<Changes, EngineError> {
        let mut files = self.definitions(start);
        if let Some(service) = start.service {
            let body = if start.base_url.is_some() { "baseUrl: ${var.baseUrl}\n" } else { "" };
            files.push(new_file(FileKind::Api, service, body)?);
        }
        let existing = self.existing(files.iter().map(|file| file.path.as_str()))?;
        if let Some(path) = existing.first() {
            let help = "`vc init` never overwrites a file; `vc check` checks the project that is here";
            return Err(refused(format!("{path} already exists"), help));
        }
        let project = self.authoring()?;
        let titles = files.iter().map(|file| format!("Create {}", file.path)).collect();
        let mut changes = self.guarded(&project, Changes { files, titles })?;
        self.scaffold(&mut changes)?;
        changes.files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }

    /// The config, and with `start.sample` the sample files, with `start`'s choices in.
    fn definitions(&self, start: &Start<'_>) -> Vec<FileChange> {
        let wanted = if start.sample { &DEFINITIONS[..] } else { &DEFINITIONS[..1] };
        let folder = self.root().file_name().map(|name| name.to_string_lossy().into_owned());
        let name = start.name.map(str::to_owned).or(folder).unwrap_or_else(|| "project".to_owned());
        wanted
            .iter()
            .map(|(path, kind, text)| created(path, &definition(*kind, &sampled(text, start), &name)))
            .collect()
    }

    /// Adds `.env.schema` when there is none, and to `.gitignore` the lines of [`IGNORED`] it lacks.
    fn scaffold(&self, changes: &mut Changes) -> Result<(), EngineError> {
        let path = |text: &str| SourcePath::new(text).map_err(internal);
        let read = self.sources.read(&[path(DEFAULT_SECRETS_SCHEMA)?, path(GITIGNORE)?]).map_err(sources_error)?;
        let text = |name: &str| read.files.iter().find(|file| file.path.as_str() == name).map(|file| file.text.clone());
        if text(DEFAULT_SECRETS_SCHEMA).is_none() {
            changes.titles.push(format!("Create {DEFAULT_SECRETS_SCHEMA}"));
            changes.files.push(created(DEFAULT_SECRETS_SCHEMA, ENV_SCHEMA));
        }
        let ignore = text(GITIGNORE);
        let missing = lacking(ignore.as_deref().unwrap_or_default());
        if !missing.is_empty() {
            changes.titles.push(format!("Ignore {} in {GITIGNORE}", missing.join(" and ")));
            changes.files.push(FileChange {
                path: GITIGNORE.to_owned(),
                after: ignored(ignore.as_deref().unwrap_or_default(), &missing),
                before: ignore,
                moved_from: None,
            });
        }
        Ok(())
    }

    /// The paths among `paths` that hold a file now.
    pub(crate) fn existing<'p>(&self, paths: impl Iterator<Item = &'p str>) -> Result<Vec<String>, EngineError> {
        let paths = paths.map(SourcePath::new).collect::<Result<Vec<_>, _>>().map_err(internal)?;
        let read = self.sources.read(&paths).map_err(sources_error)?;
        Ok(read.files.iter().map(|file| file.path.to_string()).collect())
    }
}

/// A new file at `path` holding `text`.
pub fn created(path: &str, text: &str) -> FileChange {
    FileChange { path: path.to_owned(), before: None, after: text.to_owned(), moved_from: None }
}

/// A definition file's text: its schema line, then `text` with the project's `name` in.
fn definition(kind: FileKind, text: &str, name: &str) -> String {
    format!("{}\n{}", schema_line(kind), text.replace("{project}", &write_text(name, false)))
}

/// `text` with the config's parts filled in from `start`: its vars (the sample's demo service,
/// the base URL), the commented profile var, and the default profile's ID.
fn sampled(text: &str, start: &Start<'_>) -> String {
    let sample = if start.sample { SAMPLE_VAR } else { "" };
    let base = start.base_url.map(|url| format!("  baseUrl: {}\n", write_text(url, false))).unwrap_or_default();
    let vars = format!("{sample}{base}");
    let vars = if vars.is_empty() { vars } else { format!("vars:\n{vars}\n") };
    let profile = format!("- id: {}\n", start.profile.unwrap_or("default"));
    let profile_var = PROFILE_VAR[usize::from(!start.sample)];
    text.replacen("{sampleVars}", &vars, 1).replacen("{profileVar}", profile_var, 1).replacen(
        "- id: default\n",
        &profile,
        1,
    )
}

/// The lines of [`IGNORED`] that `.gitignore`'s text `text` lacks; a leading `/` counts as the line.
fn lacking(text: &str) -> Vec<&'static str> {
    let has = |wanted: &str| text.lines().any(|line| line.trim().trim_start_matches('/') == wanted);
    IGNORED.into_iter().filter(|wanted| !has(wanted)).collect()
}

/// `.gitignore`'s text `text` with each of `lines` on a line of its own at the end.
fn ignored(text: &str, lines: &[&str]) -> String {
    let newline = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
    format!("{text}{newline}{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use vc_model::FileKind;

    use super::{CONFIG, Start, definition, ignored, lacking, sampled};

    #[test]
    fn each_ignore_line_goes_on_a_line_of_its_own() {
        assert_eq!(ignored("", &[".vibranium/", ".env"]), ".vibranium/\n.env\n");
        assert_eq!(ignored("target", &[".env"]), "target\n.env\n");
        assert_eq!(ignored("target\n", &[".vibranium/"]), "target\n.vibranium/\n");
    }

    #[test]
    fn only_the_missing_ignore_lines_are_added() {
        assert_eq!(lacking(""), [".vibranium/", ".env"]);
        assert_eq!(lacking("target\n.vibranium/\n"), [".env"]);
        assert_eq!(lacking("/.env\n  .vibranium/  \n"), Vec::<&str>::new());
        assert_eq!(lacking(".env.local\n.envrc\n"), [".vibranium/", ".env"]);
    }

    #[test]
    fn the_config_mentions_the_sample_only_with_it() {
        let sample = Start { sample: true, ..Start::default() };
        assert!(sampled(CONFIG, &sample).contains("\nvars:\n  sampleUrl: http://localhost:7421 "));
        let bare = sampled(CONFIG, &Start::default());
        assert!(!bare.contains("sample") && !bare.contains("{sampleVars}") && !bare.contains("{profileVar}"), "{bare}");
    }

    #[test]
    fn the_project_name_is_written_as_yaml_text() {
        let config = definition(FileKind::Config, "project: {project}\n", "shop: v2");
        assert!(config.starts_with("# yaml-language-server: $schema="), "{config}");
        assert!(config.ends_with("project: 'shop: v2'\n") || config.ends_with("project: \"shop: v2\"\n"), "{config}");
    }
}
