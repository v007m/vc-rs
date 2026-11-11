//! An importer's draft as definition files: a new api file with its `source`, a test file for
//! the generated tests, the config's base-URL var and the spec snapshot `openapi/<fileId>.yaml`.

use serde_json::{Map, Value, json};
use vc_interfaces::{DraftApi, DraftBefore, DraftCleanup, DraftExample, DraftGroup, DraftTest, ImportDraft};
use vc_model::{FileKind, Method};

use crate::authoring::{Authoring, refused};
use crate::create::{derived_path, header, sentence};
use crate::create_api::base_var;
use crate::edit::FileChange;
use crate::snapshot_guard::snapshot_path;
use crate::yaml_out::{body, entry, fields_item};
use crate::{Changes, EngineError, ImportRequest, import_titles};

/// The file ID the generated tests go in, under the api file's.
const TESTS: &str = "openapi";

/// The files `draft` becomes, each still to pass the compile guard, and the snapshot, which
/// the guard does not read.
pub fn drafted(
    project: &Authoring,
    draft: &ImportDraft,
    request: &ImportRequest,
) -> Result<(Changes, Vec<FileChange>), EngineError> {
    let mut files = Vec::new();
    let mut snapshots = Vec::new();
    for group in &draft.groups {
        files.extend(group_files(project, group, draft.snapshot.as_ref().map(|_| request))?);
        snapshots.extend(draft.snapshot.as_ref().map(|doc| snapshot(&group.file_id, &request.source, doc)));
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let count: usize = draft.groups.iter().map(|group| group.apis.len()).sum();
    let mut titles = vec![import_titles::heading(count, &request.source)];
    titles.extend(draft.notes.iter().map(|note| format!("{}: {}", note.at, note.message)));
    Ok((Changes { files, titles }, snapshots))
}

/// `synced` is the request whose spec the api file's `source` names; none for a draft of no spec.
/// A pasted spec (one given as text) has no file to sync from, so its api file has no `source`.
fn group_files(
    project: &Authoring,
    group: &DraftGroup,
    synced: Option<&ImportRequest>,
) -> Result<Vec<FileChange>, EngineError> {
    let id = &group.file_id;
    if project.find(FileKind::Api, id).is_some() {
        return Err(refused(
            format!("`{id}` already has an api file"),
            &format!("`vc sync {id}` merges the spec into it"),
        ));
    }
    let mut files = Vec::new();
    let mut base = group.base_url.clone().unwrap_or_default();
    if let Some((draft_var, origin)) = group.config_vars.iter().next() {
        let (var, change) = base_var(project, id, origin.as_str().unwrap_or_default())?;
        base = base.replace(&format!("${{var.{draft_var}}}"), &format!("${{var.{var}}}"));
        files.extend(change);
    }
    let fetched = synced.filter(|request| request.text.is_none());
    files.push(new_file(FileKind::Api, id, &api_file(group, &base, fetched))?);
    if !group.tests.is_empty() {
        files.push(tests_file(project, group, synced.is_some())?);
    }
    Ok(files)
}

/// The test file of `group`'s generated tests; with `snapshotted`, their schemas name the
/// spec snapshot by its path from the test file.
fn tests_file(project: &Authoring, group: &DraftGroup, snapshotted: bool) -> Result<FileChange, EngineError> {
    let tests_id = format!("{}.{TESTS}", group.file_id);
    if project.find(FileKind::Test, &tests_id).is_some() {
        return Err(refused(format!("`{tests_id}` exists"), "import without `--tests`, `--negative` and `--auth`"));
    }
    let path = derived_path(FileKind::Test, &tests_id)?.to_string();
    let up = "../".repeat(path.matches('/').count());
    let spec = snapshotted.then(|| format!("{up}{}", snapshot_path(&group.file_id)));
    new_file(FileKind::Test, &tests_id, &test_file(&tests_id, &group.tests, spec.as_deref()))
}

pub fn new_file(kind: FileKind, id: &str, text: &str) -> Result<FileChange, EngineError> {
    let path = derived_path(kind, id)?.to_string();
    Ok(FileChange { path, before: None, after: format!("{}{text}", header(kind, id)), moved_from: None })
}

pub fn api_file(group: &DraftGroup, base: &str, synced: Option<&ImportRequest>) -> String {
    let mut out = String::new();
    entry(&mut out, 0, ("id", &json!(group.file_id)));
    if let Some(name) = &group.name {
        entry(&mut out, 0, ("name", &json!(name)));
    }
    entry(&mut out, 0, ("baseUrl", &json!(base)));
    if let Some(request) = synced {
        out.push_str("source:\n");
        entry(&mut out, 2, (&request.kind, &json!(request.source)));
        for (key, values) in [("include", &request.include), ("only", &request.only)] {
            if !values.is_empty() {
                entry(&mut out, 2, (key, &json!(values)));
            }
        }
    }
    out.push_str("\napis:\n");
    group.apis.iter().for_each(|item| api(&mut out, item));
    out
}

/// One api in the house key order (D139): `id`, `name`, `method`, `path`, `vars`, `query`,
/// `headers`, `body`, `save`, `cleanup`, `operationId`.
fn api(out: &mut String, api: &DraftApi) {
    fields_item(out, 2, &api_fields(api));
}

/// The fields of `api`, in that order.
pub fn api_fields(api: &DraftApi) -> Vec<(&'static str, Value)> {
    let mut fields =
        vec![("id", json!(api.id)), ("name", json!(api.name.clone().unwrap_or_else(|| sentence(&api.id))))];
    if api.method != Method::Get {
        fields.push(("method", json!(api.method.name())));
    }
    fields.push(("path", json!(api.path)));
    if !api.vars.is_empty() {
        fields.push(("vars", json!(api.vars)));
    }
    if !api.query.is_empty() {
        fields.push(("query", json!(api.query)));
    }
    if let Some(content_type) = &api.content_type {
        fields.push(("headers", json!({ "content-type": content_type })));
    }
    fields.extend(api.body.clone().map(|body| ("body", body)));
    if !api.save.is_empty() {
        fields.push(("save", json!(api.save)));
    }
    fields.extend(api.cleanup.as_ref().map(|undo| ("cleanup", cleanup(undo))));
    fields.extend(api.operation_id.as_ref().map(|op| ("operationId", json!(op))));
    fields
}

/// `cleanup`: its `call`, and its `vars` when it has any.
fn cleanup(cleanup: &DraftCleanup) -> Value {
    let mut fields = Map::new();
    fields.insert("call".to_owned(), json!(cleanup.call));
    if !cleanup.vars.is_empty() {
        fields.insert("vars".to_owned(), json!(cleanup.vars));
    }
    Value::Object(fields)
}

/// `spec` is the spec snapshot's path from the test file, for the tests' schemas.
fn test_file(id: &str, tests: &[DraftTest], spec: Option<&str>) -> String {
    let mut out = String::new();
    entry(&mut out, 0, ("id", &json!(id)));
    out.push_str("\ntests:\n");
    for item in tests {
        test(&mut out, item, spec);
    }
    out
}

/// One test in the shape's key order: `id`, `name`, `before`, `call`, `examples`, `skip`,
/// `vars`, `auth`, `body`, `expect`. A JSON map's keys are sorted, so a row's `name` and
/// `expect.status` are written first by hand.
fn test(out: &mut String, test: &DraftTest, spec: Option<&str>) {
    let mut head = vec![("id", json!(test.id)), ("name", json!(test.name))];
    if !test.before.is_empty() {
        head.push(("before", Value::Array(test.before.iter().map(before).collect())));
    }
    head.push(("call", json!(test.call)));
    fields_item(out, 2, &head);
    examples(out, &test.examples);
    for (key, value) in middle(test) {
        entry(out, 4, (key, &value));
    }
    out.push_str("    expect:\n");
    entry(out, 6, ("status", &json!(test.status)));
    if let (Some(spec), Some(pointer)) = (spec, &test.schema) {
        entry(out, 6, ("body", &json!({ "schema": { "$file": format!("{spec}#{pointer}") } })));
    }
}

/// A `before` action: its `call`, and `keep: true` when the test's own call removes the item.
fn before(action: &DraftBefore) -> Value {
    if action.keep { json!({ "call": action.call, "keep": true }) } else { json!({ "call": action.call }) }
}

/// `examples`, each row's `name` before its `body`.
fn examples(out: &mut String, rows: &[DraftExample]) {
    if rows.is_empty() {
        return;
    }
    out.push_str("    examples:\n");
    for row in rows {
        fields_item(out, 6, &[("name", json!(row.name)), ("body", row.body.clone())]);
    }
}

/// The fields between `examples` and `expect`: `skip`, `vars`, `auth` and `body`.
fn middle(test: &DraftTest) -> Vec<(&'static str, Value)> {
    let mut fields = Vec::new();
    fields.extend(test.skip.as_ref().map(|reason| ("skip", json!({ "reason": reason }))));
    if !test.vars.is_empty() {
        fields.push(("vars", json!(test.vars)));
    }
    if test.no_auth {
        fields.push(("auth", json!("none")));
    }
    fields.extend(test.body.clone().map(|body| ("body", body)));
    fields
}

pub fn snapshot(file_id: &str, source: &str, doc: &Value) -> FileChange {
    let mut after = format!(
        "# Snapshot of {source} as OpenAPI 3.1, written by `vc import` and `vc sync`.\n\
         # `vc sync {file_id}` merges against it: commit it, and do not edit it.\n"
    );
    body(&mut after, 0, doc);
    FileChange { path: snapshot_path(file_id), before: None, after, moved_from: None }
}

#[cfg(test)]
#[path = "import_write_tests.rs"]
mod tests;
