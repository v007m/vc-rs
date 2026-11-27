//! The user config's text with settings set and removed (D712), its comments and layout kept
//! through the form edits (`edit_pointers`). A new file starts with a comment that says what it
//! is; a list setting with several values is written as a list; a removal takes the maps it
//! leaves empty with it, since an empty `defaults:` would be a setting with no value.

use serde_json::Value as Json;
use vc_model::{SourceFileId, SourcePath};
use vc_parser::{Document, Node, ScalarKind, Syntax, Value};

use crate::authoring::refused;
use crate::edit::internal;
use crate::edit_pointers::edited as pointer_edited;
use crate::yaml_out::fragment;
use crate::{EditOp, EngineError, PointerEdit};

/// One change: the dotted key, and its value or `None` to remove it.
pub type Edit<'a> = (&'a str, Option<&'a str>);

/// The first lines of a user config the engine creates.
const HEADER: &str = "# vibranium's user config: profile, defaults, reports and vars for every project,\n\
     # used only where a project's own config leaves them unset (D129).\n";

/// The settings whose value is a list, written `a,b` in a change.
const LISTS: &[&str] = &["profile", "reports.formats"];

/// What to do when the file cannot take an edit.
const HELP: &str =
    "fix the user config by hand so it is a map of settings, such as `defaults:` with `timeout: 12s` under it";

/// `before` (no file when `None`) with `edits` made, one after another.
///
/// # Errors
///
/// [`EngineError::Edit`] when the text does not parse, or is not a map an edit can go in.
pub fn edited(before: Option<&str>, edits: &[Edit<'_>]) -> Result<String, EngineError> {
    let mut text = before.map_or_else(|| HEADER.to_owned(), str::to_owned);
    for (at, &(key, value)) in edits.iter().enumerate() {
        text = one(&text, key, value).map_err(|()| {
            refused(format!("the user config: nothing was written: edit {} does not fit it", at + 1), HELP)
        })?;
    }
    Ok(text)
}

/// `text` with `key` set to `value`, or removed for `None`.
fn one(text: &str, key: &str, value: Option<&str>) -> Result<String, ()> {
    let tokens: Vec<&str> = key.split('.').collect();
    let document = parse(text)?;
    let existing = found(document.root(), &tokens);
    let Some(value) = value else {
        return if existing.is_some() { pruned(&remove(text, &tokens)?, &tokens) } else { Ok(text.to_owned()) };
    };
    let json = json(key, value);
    let root = document.root();
    if !matches!(root.value, Value::Map(_)) {
        return if null(root) { Ok(appended(text, &tokens, json)) } else { Err(()) };
    }
    match existing {
        // A list over a scalar, or a scalar over a list: the old node goes, then the new one is set.
        Some(node) if !matches!(node.value, Value::Scalar(_)) || json.is_array() => {
            one(&pruned(&remove(text, &tokens)?, &tokens)?, key, Some(value))
        }
        _ => pointer(text, &tokens, (EditOp::Set, &json.to_string())),
    }
}

/// `text`, whose root is empty, with `value` nested under `tokens` after it.
fn appended(text: &str, tokens: &[&str], value: Json) -> String {
    let nested = tokens
        .iter()
        .rev()
        .fold(value, |inner, key| Json::Object(std::iter::once(((*key).to_owned(), inner)).collect()));
    let mut out = text.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&fragment(&nested));
    out
}

/// `text` without the maps above `tokens` that the removal left empty, the deepest first.
fn pruned(text: &str, tokens: &[&str]) -> Result<String, ()> {
    let mut text = text.to_owned();
    for depth in (1..tokens.len()).rev() {
        let above = tokens.get(..depth).unwrap_or_default();
        let document = parse(&text)?;
        if !found(document.root(), above).is_some_and(empty) {
            break;
        }
        text = remove(&text, above)?;
    }
    Ok(text)
}

fn remove(text: &str, tokens: &[&str]) -> Result<String, ()> {
    pointer(text, tokens, (EditOp::Remove, ""))
}

/// `text` with one form edit made at `tokens`.
fn pointer(text: &str, tokens: &[&str], (op, value): (EditOp, &str)) -> Result<String, ()> {
    let escaped: Vec<String> = tokens.iter().map(|token| token.replace('~', "~0").replace('/', "~1")).collect();
    let edit = PointerEdit { pointer: format!("/{}", escaped.join("/")), op, value: value.to_owned() };
    pointer_edited(&path().map_err(|_| ())?, text, &[edit]).map_err(|_| ())
}

fn parse(text: &str) -> Result<Document, ()> {
    Document::parse(SourceFileId::new(0), Syntax::Yaml, text).map_err(|_| ())
}

/// The node `tokens` lead to through maps.
fn found<'n>(root: &'n Node, tokens: &[&str]) -> Option<&'n Node> {
    tokens.iter().try_fold(root, |node, token| match node.value {
        Value::Map(_) => node.get(token),
        _ => None,
    })
}

/// Whether `node` is a map with no entries, or nothing.
fn empty(node: &Node) -> bool {
    match &node.value {
        Value::Map(entries) => entries.is_empty(),
        _ => null(node),
    }
}

/// Whether `node` is null: `~`, `null` or nothing at all.
fn null(node: &Node) -> bool {
    node.as_scalar().is_some_and(|scalar| scalar.kind == ScalarKind::Null)
}

/// `value` for `key` as JSON: a list for a list setting with several values, a number for a
/// `defaults` count, else text as written.
fn json(key: &str, value: &str) -> Json {
    let names: Vec<Json> = value
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| Json::String(name.to_owned()))
        .collect();
    if LISTS.contains(&key) && names.len() > 1 {
        return Json::Array(names);
    }
    match value.trim().parse::<u64>() {
        Ok(number) if key.starts_with("defaults.") => Json::from(number),
        _ => Json::String(value.to_owned()),
    }
}

/// The name edits give the file; the user config is YAML.
fn path() -> Result<SourcePath, EngineError> {
    SourcePath::new("config.yaml").map_err(internal)
}

#[cfg(test)]
#[path = "user_settings_text_tests.rs"]
mod tests;
