//! `vc migrate v1 <folder>`: a v2 project from the Vibranium v1 project in a folder of it (D194,
//! D200). The converter is `vc_adapter_importers::migrate_v1`; the engine reads the v1 JSON
//! files, writes the result in the house style, puts a `# TODO(vc migrate)` above each item
//! that lost something, and passes it all through the compile guard (D107).

use std::collections::BTreeMap;
use std::path::Path;

use vc_adapter_importers::{MigratedFile, migrate_v1};
use vc_compiler::schema_line;
use vc_interfaces::{ImportNote, SourceFilter};
use vc_model::{DEFAULT_SECRETS_SCHEMA, FileKind, SourcePath, schema_names};

use crate::authoring::refused;
use crate::create::header;
use crate::edit::{Changes, FileChange, internal};
use crate::format::house_style;
use crate::init::{ENV_SCHEMA, created};
use crate::project::sources_error;
use crate::yaml_out::body;
use crate::{Engine, EngineError};

/// The config file, which a project to migrate into must not have yet.
const CONFIG_PATH: &str = "vibranium.config.yaml";

/// What a comment the migration leaves above an item it could not convert starts with.
const TODO: &str = "# TODO(vc migrate): ";

/// The top-level lists a file-wide TODO goes above.
const LISTS: [&str; 2] = ["tests:", "apis:"];

const READ_HELP: &str = "give the folder that holds the v1 config.json, inside the project";

impl Engine {
    /// The files of a v2 project from the v1 project in `folder` (relative to the root, `/`
    /// separators): the config, an api file per collection and a test file per scenario,
    /// passed by the compile guard; then the secret names the auth reads in `.env.schema`.
    /// The titles list each file, each TODO as `TODO(vc migrate) <path> <item>: <v1 text>`,
    /// and each note as `<where>: <note>`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when the project has a config already, the folder holds no v1
    /// project, or the guard refuses; [`EngineError::Sources`] when it cannot be read.
    pub fn migrate_v1(&self, folder: &str) -> Result<Changes, EngineError> {
        if !self.existing(std::iter::once(CONFIG_PATH))?.is_empty() {
            let help = "`vc migrate v1` writes a new project; run it in a folder without one";
            return Err(refused(format!("{CONFIG_PATH} already exists"), help));
        }
        let project = self.authoring()?;
        let migration = migrate_v1(&self.v1_files(folder)?).map_err(|error| refused(error.to_string(), READ_HELP))?;
        let mut changes = Changes::default();
        for file in &migration.files {
            changes.files.push(created(&file.path, &rendered(file)?));
            changes.titles.push(format!("Create {}", file.path));
            let todos = file.todos.iter().map(|todo| format!("TODO(vc migrate) {} {}", file.path, said(todo)));
            changes.titles.extend(todos);
        }
        changes.titles.extend(migration.notes.iter().map(said));
        let mut changes = self.guarded(&project, changes)?;
        self.declared(&migration.secrets, &mut changes)?;
        changes.files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }

    /// The JSON files under `folder`, by their path in it.
    fn v1_files(&self, folder: &str) -> Result<BTreeMap<String, String>, EngineError> {
        let folder = folder.trim_start_matches("./").trim_matches('/');
        let prefix = if folder.is_empty() || folder == "." { String::new() } else { format!("{folder}/") };
        let found = self.sources.discover(&SourceFilter::default()).map_err(sources_error)?;
        let wanted = |path: &SourcePath| path.as_str().starts_with(&prefix) && json(path.as_str());
        let paths: Vec<SourcePath> = found.assets.into_iter().filter(wanted).collect();
        let read = self.sources.read(&paths).map_err(sources_error)?;
        let files = read.files.into_iter();
        Ok(files.filter_map(|file| Some((file.path.as_str().strip_prefix(&prefix)?.to_owned(), file.text))).collect())
    }

    /// Adds the secret names `.env.schema` does not declare yet, making it when there is none.
    fn declared(&self, names: &[String], changes: &mut Changes) -> Result<(), EngineError> {
        let path = SourcePath::new(DEFAULT_SECRETS_SCHEMA).map_err(internal)?;
        let read = self.sources.read(&[path]).map_err(sources_error)?;
        let before = read.files.into_iter().next().map(|file| file.text);
        let text = before.clone().unwrap_or_else(|| ENV_SCHEMA.to_owned());
        let known = schema_names(&text).declared;
        let missing: Vec<&String> = names.iter().filter(|name| !known.contains(name)).collect();
        if missing.is_empty() && before.is_some() {
            return Ok(());
        }
        let newline = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
        let lines: String = missing.iter().flat_map(|name| [name.as_str(), "=\n"]).collect();
        changes.titles.push(format!("Declare the secrets in {DEFAULT_SECRETS_SCHEMA}"));
        let after = format!("{text}{newline}{lines}");
        changes.files.push(FileChange { path: DEFAULT_SECRETS_SCHEMA.to_owned(), before, after, moved_from: None });
        Ok(())
    }
}

/// The file's text: its header, its document in the house style, and its TODOs.
fn rendered(file: &MigratedFile) -> Result<String, EngineError> {
    let path = SourcePath::new(&file.path).map_err(internal)?;
    let kind = FileKind::from_path(&path).ok_or_else(|| internal(format!("{} is not a definition file", file.path)))?;
    let mut text = path.file_id().map_or_else(|| format!("{}\n", schema_line(kind)), |id| header(kind, &id));
    body(&mut text, 0, &file.doc);
    Ok(commented(&house_style(path, kind, &text)?, &file.todos))
}

/// `text` with a `# TODO(vc migrate)` line above the item each TODO is about; a TODO about
/// the file goes above its `tests:` or `apis:`.
fn commented(text: &str, todos: &[ImportNote]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    for todo in todos {
        let item = format!("  - id: {}", todo.at);
        let place = if todo.at.is_empty() {
            lines.iter().position(|line| LISTS.contains(&line.as_str())).map(|at| (at, ""))
        } else {
            lines.iter().position(|line| *line == item).map(|at| (at, "  "))
        };
        let (at, indent) = place.unwrap_or((lines.len(), ""));
        lines.insert(at, format!("{indent}{TODO}{}", one_line(&todo.message)));
    }
    lines.join("\n") + "\n"
}

/// A note or TODO for the report: `<where>: <text>`, or the text alone for the whole file.
fn said(note: &ImportNote) -> String {
    let text = one_line(&note.message);
    if note.at.is_empty() { text } else { format!("{}: {text}", note.at) }
}

/// `text` on one line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `path` names a `.json` file, in any case.
fn json(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

#[cfg(test)]
mod tests {
    use vc_interfaces::ImportNote;

    use super::commented;

    fn todo(at: &str, message: &str) -> ImportNote {
        ImportNote { at: at.to_owned(), message: message.to_owned() }
    }

    #[test]
    fn a_todo_goes_above_its_item_or_the_file_list() {
        let text = "name: a\ntests:\n  - id: getsUser\n    call: a#b\n";
        let todos = [todo("getsUser", "scripts.after-endpoint: x\n  y"), todo("", "scripts.before-scenario: z")];
        let expected = "name: a\n# TODO(vc migrate): scripts.before-scenario: z\ntests:\n  \
                        # TODO(vc migrate): scripts.after-endpoint: x y\n  - id: getsUser\n    call: a#b\n";
        assert_eq!(commented(text, &todos), expected);
    }
}
