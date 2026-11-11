//! `$file` request bodies (D272). [`body_files`] finds the `$file`s that the given actions'
//! bodies write, each literal path resolved from the folder of the file that writes it, so the
//! engine can read them before a run. The bodies stay as written. A whole-body `$file` sits at
//! the empty pointer, and a map's `$file` part at `/name` or `/name/index` (D274), whatever the
//! `content-type`: the executor decides at send time whether a part is sent as a file. A path
//! that holds `${…}` is not read, and the executor fails it with V5111.

use vc_model::{ActionId, ActionKind, BodySpec, Program, SourcePath, SourceSpan, Template};

use crate::link::folder;

/// Where a `$file` sits: the action that sends the body, and the JSON pointer to the file.
pub type FileSlot = (ActionId, String);

/// A `$file`'s project path, or why it is not read.
pub type FilePath = Result<SourcePath, Unreadable>;

/// Why a `$file` written in a body is not read before the run.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unreadable {
    /// Its path holds `${…}` references, so only the run could know it (D272).
    Templated,
    /// Its path leads out of the project, or is not a project path.
    Refused(String),
}

/// The `$file`s in the bodies that `actions` send, each with its project path or why it is not
/// read (D272).
#[must_use]
pub fn body_files(program: &Program, actions: impl IntoIterator<Item = ActionId>) -> Vec<(FileSlot, FilePath)> {
    let mut found = Vec::new();
    for id in actions {
        let Some((body, span)) = sent(program, id) else { continue };
        let folder = program.files.get(span.file).map(|file| folder(file.path.as_str()));
        let folder = folder.unwrap_or_default();
        found.extend(written(body).into_iter().map(|(pointer, path)| ((id, pointer), resolved(&folder, path))));
    }
    found
}

/// The body action `id` sends and where it is written, if it sends one: an inline request's,
/// or a call's (the step's own body, else its api's).
fn sent(program: &Program, id: ActionId) -> Option<(&BodySpec, SourceSpan)> {
    let action = usize::try_from(id.index()).ok().and_then(|at| program.actions.get(at))?;
    match &action.kind {
        ActionKind::Inline(inline) => Some((&inline.body, action.span)),
        ActionKind::Call(api) => {
            let api = program.apis.get(api)?;
            Some(action.body.as_ref().map_or((&api.body, api.span), |body| (body, action.span)))
        }
        _ => None,
    }
}

/// Each `$file` path `body` writes, with its pointer.
fn written(body: &BodySpec) -> Vec<(String, &Template)> {
    match body {
        BodySpec::File(path) => vec![(String::new(), path)],
        BodySpec::Json(value) | BodySpec::Extend { changes: value, .. } => vc_model::file_parts(value),
        _ => Vec::new(),
    }
}

/// The written `path` resolved from `folder`, or why it is not read.
fn resolved(folder: &[&str], path: &Template) -> FilePath {
    let written = path.as_literal().ok_or(Unreadable::Templated)?;
    let refused = || Unreadable::Refused("it leads out of the project".to_owned());
    let path = vc_model::resolve_path(folder, written).ok_or_else(refused)?;
    SourcePath::new(&path).map_err(|error| Unreadable::Refused(error.to_string()))
}

#[cfg(test)]
#[path = "body_files_tests.rs"]
mod tests;
