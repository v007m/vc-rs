//! A request the web UI's API client edits before it is saved (D245): the api as its file
//! writes it, for the form to start from, and pointer edits to that file which a call is sent
//! with in place of the file. Nothing is written; saving the draft is an `EditFile`.

use serde_json::{Map, Number, Value as Json};
use vc_model::{SourceFileId, SourcePath};
use vc_parser::{Document, Node, ScalarKind, Syntax, Value};

use crate::authoring::refused;
use crate::call::unknown;
use crate::edit_pointers::edited;
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
    /// The digest `ReadFile` or [`Engine::written_api`] gave for it.
    pub digest: String,
    /// Made in order, as `EditFile` makes them.
    pub edits: Vec<PointerEdit>,
}

/// An api as its file writes it, for a form to edit by pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WrittenApi {
    /// The api file, relative to the project root.
    pub path: String,
    /// Where the api sits in its file, `/apis/2`; the pointers of its edits start with it.
    pub pointer: String,
    /// The file's digest, for the edits and the save.
    pub digest: String,
    /// The api's YAML as JSON text, templates as written, masked as a run's values are.
    pub json: String,
}

impl Engine {
    /// The api `id` (`users#get`) as its file writes it.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors; [`EngineError::Selection`] when no
    /// api has that ID; the errors of [`Engine::read_file`] for its file; [`EngineError::Edit`]
    /// when the file no longer parses or holds the api.
    pub async fn written_api(&self, id: &str) -> Result<WrittenApi, EngineError> {
        let (program, _) = self.program()?;
        let api = program.apis.values().find(|api| program.str(api.id.sym()) == Some(id));
        let api = api.ok_or_else(|| unknown(&program, id))?;
        let path = program.files.get(api.span.file).map(|file| file.path.clone());
        let path = path.ok_or_else(|| EngineError::Internal { reason: format!("api `{id}` has no file") })?;
        let opened = self.read_file(path.as_str())?;
        let local = id.rsplit_once('#').map_or(id, |(_, local)| local);
        let (at, json) = written(&path, &opened.text, local)?;
        let json = self.masked_text(&json.to_string()).await;
        Ok(WrittenApi { path: path.to_string(), pointer: format!("/apis/{at}"), digest: opened.digest, json })
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

/// Where the api `local` sits in the file `text`, and its node as JSON.
fn written(path: &SourcePath, text: &str, local: &str) -> Result<(usize, Json), EngineError> {
    let help = "fix it in the editor first; `vc check` shows where";
    let document = Document::parse(SourceFileId::new(0), Syntax::of(path), text)
        .map_err(|_| refused(format!("{path} does not parse"), help))?;
    let apis = document.root().get("apis").map(Node::items).unwrap_or_default();
    let found = apis.iter().enumerate().find(|(_, api)| api.get("id").and_then(Node::as_str) == Some(local));
    let (at, node) = found.ok_or_else(|| refused(format!("{path} has no api `{local}`"), help))?;
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
