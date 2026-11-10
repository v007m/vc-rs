//! A sync plan's steps as edits to the api file, made one at a time so that each finds its api
//! in the text the step before left; and the api file's apis as sync sees them.

use serde_json::Value;
use vc_diagnostics::TextEdit;
use vc_parser::{Document, Entry, Node, ParserError, ScalarKind};

use crate::EngineError;
use crate::authoring::{item, refused};
use crate::import_write::api_fields;
use crate::sync_plan::{Mine, Owned, Step};
use crate::yaml_out::{entry, fragment};

/// The apis of the api file whose root is `root`, in file order.
pub fn mine(root: &Node) -> Vec<Mine> {
    root.get("apis").map(Node::items).unwrap_or_default().iter().filter_map(one).collect()
}

fn one(api: &Node) -> Option<Mine> {
    let text = |key: &str| api.get(key).and_then(Node::as_str).map(str::to_owned);
    let query = entries(api, "query").iter().filter_map(|e| Some((e.key.clone(), scalar(&e.value)?))).collect();
    let content_type = header(api).and_then(|e| scalar(&e.value));
    let method = text("method").unwrap_or_else(|| "GET".to_owned()).to_uppercase();
    Some(Mine {
        id: text("id")?,
        operation_id: text("operationId"),
        owned: Owned { method, path: text("path").unwrap_or_default(), query, content_type },
        vars: entries(api, "vars").iter().map(|e| e.key.clone()).collect(),
        deprecated: api.get("deprecated").and_then(Node::as_scalar).is_some_and(|flag| flag.text == "true"),
    })
}

fn entries<'n>(api: &'n Node, map: &str) -> &'n [Entry] {
    api.get(map).map(Node::entries).unwrap_or_default()
}

/// The api's `content-type` header, in any case.
fn header(api: &Node) -> Option<&Entry> {
    entries(api, "headers").iter().find(|e| e.key.eq_ignore_ascii_case("content-type"))
}

fn scalar(node: &Node) -> Option<String> {
    node.as_scalar().map(|scalar| scalar.text.clone())
}

/// `document` with every step made.
///
/// # Errors
///
/// [`EngineError::Edit`] when a step cannot be made in the file as written, such as removing
/// an entry from a `{..}` map.
pub fn apply(document: Document, steps: &[Step]) -> Result<Document, EngineError> {
    steps.iter().try_fold(document, |document, step| {
        let cannot = |error: ParserError| refused(format!("cannot sync {}: {error}", name(step)), HELP);
        let edit = edit(&document, step).map_err(cannot)?;
        document.apply(&[edit]).map_err(cannot)
    })
}

const HELP: &str = "make this change by hand, then run `vc sync` again";

fn name(step: &Step) -> &str {
    match step {
        Step::Set { api, .. } | Step::Put { api, .. } | Step::Remove { api, .. } | Step::Deprecate { api } => api,
        Step::Append(draft) => &draft.id,
    }
}

fn edit(document: &Document, step: &Step) -> Result<TextEdit, ParserError> {
    let root = document.root();
    let api = |id: &str| item(root, "apis", id).ok_or(GONE);
    match step {
        Step::Set { api: id, key, value } => set(document, api(id)?, (key, value)),
        Step::Put { api: id, map, key, value } => put(document, api(id)?, (map, key, value)),
        Step::Remove { api: id, map, key } => remove(document, api(id)?, (map, key)),
        Step::Deprecate { api: id } => match api(id)?.get("deprecated") {
            Some(flag) => document.set_typed(flag, "true", ScalarKind::Bool),
            None => document.insert_yaml(api(id)?, "deprecated", "true"),
        },
        Step::Append(draft) => {
            let mut yaml = String::new();
            for (key, value) in &api_fields(draft) {
                entry(&mut yaml, 0, (key, value));
            }
            document.insert_item(root.get("apis").ok_or(GONE)?, &yaml)
        }
    }
}

const GONE: ParserError = ParserError::WrongNode { expected: "the api", found: "nothing" };

fn set(document: &Document, api: &Node, (key, value): (&str, &str)) -> Result<TextEdit, ParserError> {
    api.get(key).map_or_else(
        || document.insert_entry(api, key, value),
        |node| document.set_typed(node, value, ScalarKind::String),
    )
}

/// `key` in the map `map` of `api` (any case for a header), set or added.
fn put(document: &Document, api: &Node, (map, key, value): (&str, &str, &Value)) -> Result<TextEdit, ParserError> {
    let Some(host) = api.get(map) else {
        let mut yaml = String::new();
        entry(&mut yaml, 0, (key, value));
        return document.insert_yaml(api, map, &yaml);
    };
    match (found(host, map, key), value) {
        (Some(old), Value::String(text)) => document.set_typed(&old.value, text, ScalarKind::String),
        (Some(old), other) => document.set_scalar(&old.value, &fragment(other)),
        (None, _) => document.insert_yaml(host, key, &fragment(value)).or_else(|_| {
            document.insert_flow_entry(
                host,
                key,
                value.as_str().map_or_else(|| fragment(value), str::to_owned).as_str(),
            )
        }),
    }
}

/// `key` taken out of the map `map` of `api`, or the whole map when it is the last entry.
fn remove(document: &Document, api: &Node, (map, key): (&str, &str)) -> Result<TextEdit, ParserError> {
    let host = api.get(map).ok_or(GONE)?;
    let old = found(host, map, key).ok_or(GONE)?;
    if host.entries().len() == 1 { document.remove(host) } else { document.remove(&old.value) }
}

fn found<'n>(host: &'n Node, map: &str, key: &str) -> Option<&'n Entry> {
    let headers = map == "headers";
    host.entries().iter().find(|e| e.key == key || (headers && e.key.eq_ignore_ascii_case(key)))
}

#[cfg(test)]
#[path = "sync_apply_tests.rs"]
mod tests;
