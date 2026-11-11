//! The web UI's form edits (M9b): values set, removed or inserted in one config, api, test or
//! load file by JSON pointer (RFC 6901), keeping its comments and layout (`parser::edit`). The
//! path rule and the digest check are `ReadFile`'s; the result passes the compile guard (D107)
//! before anything is written, and is written only when asked, so a form can preview the diff.
//! A refusal names the edit by its place in the list, never its pointer or value.

use serde_json::Value as Json;
use vc_diagnostics::TextEdit;
use vc_model::{SourceFileId, SourcePath};
use vc_parser::{Document, Node, ParserError, ScalarKind, Syntax, Value};

use crate::authoring::refused;
use crate::yaml_out::fragment;
use crate::{CheckReport, Engine, EngineError, FileChange, SaveFile};

const POINTER_HELP: &str = "point at a key or list item, such as /defaults/timeout or /profiles/0/default; \
     add a list item at /<list>/-";

/// What an edit does at its pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditOp {
    /// Replaces the scalar there, or adds the key, with any maps above it that are missing.
    Set,
    /// Removes the map entry or list item there, with its lines.
    Remove,
    /// Adds a new key to a map, or a new last item to a list (`-`).
    Insert,
}

/// One edit: where, what, and its value as JSON text (a string, number, `true`/`false`, a list
/// or a map; unused by a remove).
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps from their own requests; every user is in this repo (D160)"
)]
pub struct PointerEdit {
    /// RFC 6901, such as `/profiles/0/default`.
    pub pointer: String,
    /// What to do there.
    pub op: EditOp,
    /// The value as JSON text; empty for a remove.
    pub value: String,
}

/// The edits to make to one file, in order, each pointer read against the file as the edits
/// before it left it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps from their own requests; every user is in this repo (D160)"
)]
pub struct PointerEdits {
    /// Relative to the project root, as `ReadFile` takes it.
    pub path: String,
    /// The digest `ReadFile` gave; a file changed since is not edited (`EngineError::Stale`).
    pub digest: String,
    /// The edits, made in order.
    pub edits: Vec<PointerEdit>,
    /// Whether to write the file; otherwise it is only previewed.
    pub write: bool,
}

/// The file after the edits.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EditedFile {
    /// The file's text after the edits, unmasked: for the engine's own users, never a response.
    pub text: String,
    /// The change as a unified diff, masked as a run's values are; empty when the edits change
    /// nothing.
    pub diff: String,
    /// The digest of the file on disk: the new one once written, else the one given.
    pub digest: String,
    /// The project's check after the write; `None` when nothing was written.
    pub check: Option<Result<CheckReport, EngineError>>,
}

impl Engine {
    /// `request`'s edits made to its file: previewed, or written when it asks.
    ///
    /// # Errors
    ///
    /// As [`Engine::read_file`] for the path (the root `.env.schema` is not YAML, so it is
    /// refused too); [`EngineError::Stale`] when the file changed since it was read;
    /// [`EngineError::Edit`] for an edit that does not fit the file, or a result that adds an
    /// error; as [`Engine::save_file`] for the write.
    pub async fn edit_by_pointer(&self, request: &PointerEdits) -> Result<EditedFile, EngineError> {
        let path = self.editor_path(&request.path)?;
        let opened = self.read_file(path.as_str())?;
        if opened.digest != request.digest {
            let reason = format!("{path}: nothing was written: it changed since it was opened");
            return Err(EngineError::Stale { reason });
        }
        let text = edited(&path, &opened.text, &request.edits)?;
        let save = SaveFile { path: path.to_string(), text, expected_digest: Some(opened.digest) };
        let changes = self.edit_file(&save)?;
        let diff = match changes.files.first().map(FileChange::diff) {
            Some(diff) if !diff.is_empty() => self.masked_text(&diff).await,
            _ => String::new(),
        };
        let digest = request.digest.clone();
        if !request.write || changes.is_empty() {
            return Ok(EditedFile { text: save.text, diff, digest, check: None });
        }
        let saved = self.save_file(&save)?;
        Ok(EditedFile { text: save.text, diff, digest: saved.digest, check: Some(saved.check) })
    }
}

/// `text` with `edits` made, one after another.
fn edited(path: &SourcePath, text: &str, edits: &[PointerEdit]) -> Result<String, EngineError> {
    let file = SourceFileId::new(0);
    let mut document = Document::parse(file, Syntax::of(path), text)
        .map_err(|_| refused(format!("{path} does not parse"), "fix it in the editor first; `vc check` shows where"))?;
    for (at, edit) in edits.iter().enumerate() {
        document = applied(&document, edit)
            .map_err(|why| refused(format!("{path}: nothing was written: edit {} {why}", at + 1), POINTER_HELP))?;
    }
    Ok(document.text().to_owned())
}

/// `document` with `edit` made, or why it cannot be, without its pointer or value.
fn applied(document: &Document, edit: &PointerEdit) -> Result<Document, String> {
    let tokens = tokens(&edit.pointer)?;
    let made = match edit.op {
        EditOp::Remove => document.remove(found(document.root(), &tokens)?),
        EditOp::Set => set(document, &tokens, &json(&edit.value)?)?,
        EditOp::Insert => insert(document, &tokens, &json(&edit.value)?)?,
    };
    made.and_then(|made| document.apply(&[made])).map_err(|error| told(&error))
}

/// The pointer's reference tokens, `~1` read as `/` and `~0` as `~`; the whole file is not one
/// edit's place.
fn tokens(pointer: &str) -> Result<Vec<String>, String> {
    let rest = pointer.strip_prefix('/').ok_or("needs a pointer starting with /")?;
    Ok(rest.split('/').map(|token| token.replace("~1", "/").replace("~0", "~")).collect())
}

fn json(text: &str) -> Result<Json, String> {
    serde_json::from_str(text).map_err(|_| "needs its value as JSON text".to_owned())
}

/// The node `tokens` lead to.
fn found<'n>(root: &'n Node, tokens: &[String]) -> Result<&'n Node, String> {
    tokens.iter().try_fold(root, |node, token| child(node, token).ok_or_else(|| "points at nothing".to_owned()))
}

fn child<'n>(node: &'n Node, token: &str) -> Option<&'n Node> {
    match &node.value {
        Value::Map(_) => node.get(token),
        Value::Seq(items) => token.parse::<usize>().ok().and_then(|at| items.get(at)),
        _ => None,
    }
}

type Made = Result<Result<TextEdit, ParserError>, String>;

/// Replaces the scalar `tokens` lead to; where they lead past the end of a map, adds the first
/// missing key with the maps the rest name around `value`.
fn set(document: &Document, tokens: &[String], value: &Json) -> Made {
    let (mut node, mut depth) = (document.root(), 0);
    while let Some(next) = tokens.get(depth).and_then(|token| child(node, token)) {
        (node, depth) = (next, depth + 1);
    }
    let Some((key, rest)) = tokens.get(depth..).and_then(<[String]>::split_first) else {
        let (text, kind) = scalar(value).ok_or("sets a string, number or true/false; remove a list or map first")?;
        return Ok(document.set_typed(node, &text, kind));
    };
    let nested = rest
        .iter()
        .rev()
        .fold(value.clone(), |inner, key| Json::Object(std::iter::once((key.clone(), inner)).collect()));
    add(document, node, (key, &nested))
}

/// Adds `value` at `tokens`: a new key of a map, or a new last item of a list (`-`).
fn insert(document: &Document, tokens: &[String], value: &Json) -> Made {
    let (last, above) = tokens.split_last().ok_or("needs a place to insert at")?;
    let parent = found(document.root(), above)?;
    match &parent.value {
        Value::Seq(items) if last == "-" || last.parse::<usize>() == Ok(items.len()) => {
            Ok(document.insert_item(parent, &fragment(value)))
        }
        Value::Seq(_) => Err("adds list items only at the end, `-`".to_owned()),
        _ => add(document, parent, (last, value)),
    }
}

/// `key: value` as the last entry of the map `map`, block or flow.
fn add(document: &Document, map: &Node, (key, value): (&str, &Json)) -> Made {
    if !matches!(map.value, Value::Map(_)) {
        return Err("points inside a value that is not a map or list".to_owned());
    }
    let block = document.insert_yaml(map, key, &fragment(value));
    let flow = || scalar(value).map(|(text, _)| document.insert_flow_entry(map, key, &text));
    Ok(match block {
        Err(ParserError::WrongNode { .. }) => flow().unwrap_or(block),
        block => block,
    })
}

/// A JSON scalar as text, with the kind it is written as.
fn scalar(value: &Json) -> Option<(String, ScalarKind)> {
    match value {
        Json::String(text) => Some((text.clone(), ScalarKind::String)),
        Json::Bool(flag) => Some((flag.to_string(), ScalarKind::Bool)),
        Json::Number(number) if number.is_f64() => Some((number.to_string(), ScalarKind::Float)),
        Json::Number(number) => Some((number.to_string(), ScalarKind::Int)),
        Json::Null | Json::Array(_) | Json::Object(_) => None,
    }
}

/// What a parser refusal means for the edit, without the text it was given.
fn told(error: &ParserError) -> String {
    match error {
        ParserError::WrongNode { expected, found } => format!("needs {expected}, found {found}"),
        ParserError::NotOfKind { kind, .. } => format!("has a value that cannot be written as {kind}"),
        ParserError::DuplicateKey(_) => "adds a key the map has already; set it instead".to_owned(),
        ParserError::Unparsable(_) => "would leave the file unparsable".to_owned(),
        _ => "does not fit the file".to_owned(),
    }
}

#[cfg(test)]
#[path = "edit_pointers_tests.rs"]
mod tests;
