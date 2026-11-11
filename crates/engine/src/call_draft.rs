//! A request the web UI's API client edits before it is saved (D245): the api or flow test as
//! its file writes it, for the form to start from (D251), and pointer edits to that file which a call is sent
//! with in place of the file. Nothing is written; saving the draft is an `EditFile`.

use serde_json::{Map, Number, Value as Json};
use vc_model::{Program, SourceFileId, SourcePath};
use vc_parser::{Document, Node, ScalarKind, Syntax, Value};

use crate::authoring::refused;
use crate::edit_pointers::edited;
use crate::explain::did_you_mean;
use crate::{Engine, EngineError, FileChange, PointerEdit};

/// Pointer edits to an api file, made over the file as it was opened at `digest`; a call
/// with them sends the edited api without writing the file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps from their own requests; every user is in this repo (D160)"
)]
pub struct CallDraft {
    /// The api file, relative to the project root.
    pub path: String,
    /// The digest `ReadFile` or [`Engine::written_definition`] gave for it.
    pub digest: String,
    /// Made in order, as `EditFile` makes them.
    pub edits: Vec<PointerEdit>,
}

/// An api or test as its file writes it, for a form to edit by pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WrittenNode {
    /// Its file, relative to the project root.
    pub path: String,
    /// Where it sits in its file, `/apis/2` or `/tests/0`; the pointers of its edits start
    /// with it.
    pub pointer: String,
    /// The file's digest, for the edits and the save.
    pub digest: String,
    /// Its YAML as JSON text, templates as written, masked as a run's values are.
    pub json: String,
}

impl Engine {
    /// The api or test `id` (`users#get`, `users.crud#getsCreatedUser`) as its file writes it.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors; [`EngineError::Selection`] when no
    /// api or test has that ID; the errors of [`Engine::read_file`] for its file;
    /// [`EngineError::Edit`] when the file no longer parses or holds it.
    pub async fn written_definition(&self, id: &str) -> Result<WrittenNode, EngineError> {
        let (program, _) = self.program()?;
        let (file, list) = defined(&program, id).ok_or_else(|| undefined(&program, id))?;
        let path = program.files.get(file).map(|file| file.path.clone());
        let path = path.ok_or_else(|| EngineError::Internal { reason: format!("`{id}` has no file") })?;
        let opened = self.read_file(path.as_str())?;
        let local = id.rsplit_once('#').map_or(id, |(_, local)| local);
        let (at, json) = written(&path, &opened.text, (list, local))?;
        let json = self.masked_text(&json.to_string()).await;
        Ok(WrittenNode { path: path.to_string(), pointer: format!("/{list}/{at}"), digest: opened.digest, json })
    }

    /// The file `draft` edits, as the change a call compiles in its place.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when the path is not one the editor opens, or an edit does not
    /// apply; [`EngineError::Stale`] when the file changed since it was opened; the errors of
    /// [`Engine::read_file`].
    pub(crate) fn draft_change(&self, draft: &CallDraft) -> Result<FileChange, EngineError> {
        let path = self.editor_path(&draft.path)?;
        let opened = self.read_file(path.as_str())?;
        if opened.digest != draft.digest {
            return Err(EngineError::Stale { reason: format!("{path}: changed since it was opened; reload it") });
        }
        let text = edited(&path, &opened.text, &draft.edits)?;
        self.unsaved(path.as_str(), &text)
    }
}

/// The file of the api or test `id`, and the list its file keeps it in.
fn defined(program: &Program, id: &str) -> Option<(SourceFileId, &'static str)> {
    let named = |sym| program.str(sym) == Some(id);
    let api = program.apis.values().find(|api| named(api.id.sym())).map(|api| (api.span.file, "apis"));
    api.or_else(|| program.tests.values().find(|test| named(test.id.sym())).map(|test| (test.span.file, "tests")))
}

/// No api or test is `id`; the help names the closest one.
fn undefined(program: &Program, id: &str) -> EngineError {
    let apis = program.apis.keys().map(|api| api.sym());
    let names = apis.chain(program.tests.keys().map(|test| test.sym())).filter_map(|sym| program.str(sym));
    let reason = format!("no api or test `{id}`");
    EngineError::Selection { reason, help: did_you_mean(&vc_planner::closest(id, names)) }
}

/// Where the item `local` sits in the list `list` of the file `text`, and its node as JSON.
fn written(path: &SourcePath, text: &str, (list, local): (&str, &str)) -> Result<(usize, Json), EngineError> {
    let help = "fix it in the editor first; `vc check` shows where";
    let document = Document::parse(SourceFileId::new(0), Syntax::of(path), text)
        .map_err(|_| refused(format!("{path} does not parse"), help))?;
    let items = document.root().get(list).map(Node::items).unwrap_or_default();
    let found = items.iter().enumerate().find(|(_, item)| item.get("id").and_then(Node::as_str) == Some(local));
    let (at, node) = found.ok_or_else(|| refused(format!("{path} has no `{local}` in {list}"), help))?;
    Ok((at, node_json(node)))
}

/// A YAML node as the JSON value it reads as; a scalar JSON cannot hold, such as `.inf` or
/// `0x1F`, is its text.
fn node_json(node: &Node) -> Json {
    match &node.value {
        Value::Scalar(scalar) => {
            let text = || Json::String(scalar.text.clone());
            match scalar.kind {
                ScalarKind::Null => Json::Null,
                ScalarKind::Bool => Json::Bool(scalar.text.eq_ignore_ascii_case("true")),
                ScalarKind::Int => scalar.text.parse::<i64>().map_or_else(|_| text(), Json::from),
                ScalarKind::Float => {
                    scalar.text.parse::<f64>().ok().and_then(Number::from_f64).map_or_else(text, Json::Number)
                }
                ScalarKind::String => text(),
            }
        }
        Value::Seq(items) => Json::Array(items.iter().map(node_json).collect()),
        Value::Map(entries) => Json::Object(
            entries.iter().map(|entry| (entry.key.clone(), node_json(&entry.value))).collect::<Map<_, _>>(),
        ),
    }
}

#[cfg(test)]
#[path = "call_draft_tests.rs"]
mod tests;
