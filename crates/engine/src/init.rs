//! `vc init`: a new project that checks clean and runs green offline (D89, D175): the config
//! with a `default` profile, the sample api and test files against the built-in demo service,
//! `.env.schema`, and `.vibranium/` in `.gitignore`. It never overwrites a definition file.

use vc_compiler::schema_line;
use vc_model::{DEFAULT_SECRETS_SCHEMA, FileKind, SourcePath};
use vc_parser::write_text;

use crate::authoring::refused;
use crate::edit::{Changes, FileChange, internal};
use crate::project::sources_error;
use crate::{Engine, EngineError};

/// The config, with `{project}` for the project's name.
const CONFIG: &str = include_str!("init/vibranium.config.yaml");
const SAMPLE_API: &str = include_str!("init/sample.api.yaml");
const SAMPLE_TESTS: &str = include_str!("init/basics.test.yaml");
pub const ENV_SCHEMA: &str = include_str!("init/env.schema");
/// The config's sample-only parts: placeholder, text with the sample, text without it.
const SAMPLE_PARTS: [(&str, &str, &str); 2] = [
    ("{sampleVars}", "vars:\n  sampleUrl: http://localhost:7421    # the demo service used by services/sample\n\n", ""),
    ("{profileVar}", "sampleUrl: https://sample.staging.example", "baseUrl: https://api.staging.example"),
];
const GITIGNORE: &str = ".gitignore";
const IGNORED: &str = ".vibranium/";

/// The definition files `vc init` writes: path, kind and text after the schema line.
const DEFINITIONS: [(&str, FileKind, &str); 3] = [
    ("vibranium.config.yaml", FileKind::Config, CONFIG),
    ("services/sample/sample.api.yaml", FileKind::Api, SAMPLE_API),
    ("services/sample/basics.test.yaml", FileKind::Test, SAMPLE_TESTS),
];

/// The empty folders of the conventional layout, which the app creates.
pub const INIT_FOLDERS: [&str; 5] = ["schemas", "datasets", "hooks", "openapi", "load"];

impl Engine {
    /// The files of a new project in the engine's root: the config, and with `sample` the
    /// sample api and test files, passed by the compile guard; then `.env.schema` when there is
    /// none, and `.gitignore` with `.vibranium/` added when it lacks it.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when a definition file it would write exists (a project is here
    /// already) or the guard refuses; [`EngineError::Sources`] when the folder cannot be read.
    pub fn init(&self, sample: bool) -> Result<Changes, EngineError> {
        let wanted = if sample { &DEFINITIONS[..] } else { &DEFINITIONS[..1] };
        let existing = self.existing(wanted.iter().map(|(path, _, _)| *path))?;
        if let Some(path) = existing.first() {
            let help = "`vc init` never overwrites a file; `vc check` checks the project that is here";
            return Err(refused(format!("{path} already exists"), help));
        }
        let project = self.authoring()?;
        let name =
            self.root().file_name().map_or_else(|| "project".to_owned(), |name| name.to_string_lossy().into_owned());
        let files = wanted
            .iter()
            .map(|(path, kind, text)| created(path, &definition(*kind, &sampled(text, sample), &name)))
            .collect();
        let titles = wanted.iter().map(|(path, _, _)| format!("Create {path}")).collect();
        let mut changes = self.guarded(&project, Changes { files, titles })?;
        self.scaffold(&mut changes)?;
        changes.files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }

    /// Adds `.env.schema` when there is none and `.vibranium/` to `.gitignore` when it lacks it.
    fn scaffold(&self, changes: &mut Changes) -> Result<(), EngineError> {
        let path = |text: &str| SourcePath::new(text).map_err(internal);
        let read = self.sources.read(&[path(DEFAULT_SECRETS_SCHEMA)?, path(GITIGNORE)?]).map_err(sources_error)?;
        let text = |name: &str| read.files.iter().find(|file| file.path.as_str() == name).map(|file| file.text.clone());
        if text(DEFAULT_SECRETS_SCHEMA).is_none() {
            changes.titles.push(format!("Create {DEFAULT_SECRETS_SCHEMA}"));
            changes.files.push(created(DEFAULT_SECRETS_SCHEMA, ENV_SCHEMA));
        }
        let ignore = text(GITIGNORE);
        if !ignore.as_deref().is_some_and(|text| text.lines().any(|line| line.trim() == IGNORED)) {
            changes.titles.push(format!("Ignore {IGNORED} in {GITIGNORE}"));
            changes.files.push(FileChange {
                path: GITIGNORE.to_owned(),
                after: ignored(ignore.as_deref().unwrap_or_default()),
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

/// `text` with the config's sample-only parts filled in, or left out without the sample.
fn sampled(text: &str, sample: bool) -> String {
    SAMPLE_PARTS.iter().fold(text.to_owned(), |text, (placeholder, with, without)| {
        text.replace(placeholder, if sample { with } else { without })
    })
}

/// `.gitignore`'s text `text` with `.vibranium/` on a line of its own at the end.
fn ignored(text: &str) -> String {
    let newline = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
    format!("{text}{newline}{IGNORED}\n")
}

#[cfg(test)]
mod tests {
    use vc_model::FileKind;

    use super::{CONFIG, definition, ignored, sampled};

    #[test]
    fn the_ignore_line_goes_on_a_line_of_its_own() {
        assert_eq!(ignored(""), ".vibranium/\n");
        assert_eq!(ignored("target"), "target\n.vibranium/\n");
        assert_eq!(ignored("target\n"), "target\n.vibranium/\n");
    }

    #[test]
    fn the_config_mentions_the_sample_only_with_it() {
        assert!(sampled(CONFIG, true).contains("\nvars:\n  sampleUrl: http://localhost:7421 "));
        let bare = sampled(CONFIG, false);
        assert!(!bare.contains("sample") && !bare.contains("{sampleVars}") && !bare.contains("{profileVar}"), "{bare}");
    }

    #[test]
    fn the_project_name_is_written_as_yaml_text() {
        let config = definition(FileKind::Config, "project: {project}\n", "shop: v2");
        assert!(config.starts_with("# yaml-language-server: $schema="), "{config}");
        assert!(config.ends_with("project: 'shop: v2'\n") || config.ends_with("project: \"shop: v2\"\n"), "{config}");
    }
}
