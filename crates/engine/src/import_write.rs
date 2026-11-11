//! An importer's draft as definition files: a new api file with its `source`, a test file for
//! the generated tests, the config's base-URL var and the spec snapshot `openapi/<fileId>.yaml`.

use serde_json::{Value, json};
use vc_interfaces::{DraftApi, DraftGroup, DraftTest, ImportDraft};
use vc_model::{FileKind, Method};

use crate::authoring::{Authoring, refused};
use crate::create::{derived_path, header, sentence};
use crate::create_api::base_var;
use crate::edit::FileChange;
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
    files.push(new_file(FileKind::Api, id, &api_file(group, &base, synced))?);
    if !group.tests.is_empty() {
        let tests_id = format!("{id}.{TESTS}");
        if project.find(FileKind::Test, &tests_id).is_some() {
            return Err(refused(format!("`{tests_id}` exists"), "import without `--tests`, `--negative` and `--auth`"));
        }
        files.push(new_file(FileKind::Test, &tests_id, &test_file(&tests_id, &group.tests))?);
    }
    Ok(files)
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
        if !request.include.is_empty() {
            entry(&mut out, 2, ("include", &json!(request.include)));
        }
    }
    out.push_str("\napis:\n");
    group.apis.iter().for_each(|item| api(&mut out, item));
    out
}

/// One api in the house key order (D139): `id`, `name`, `method`, `path`, `vars`, `query`,
/// `headers`, `body`, `operationId`.
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
    fields.extend(api.operation_id.as_ref().map(|op| ("operationId", json!(op))));
    fields
}

fn test_file(id: &str, tests: &[DraftTest]) -> String {
    let mut out = String::new();
    entry(&mut out, 0, ("id", &json!(id)));
    out.push_str("\ntests:\n");
    for item in tests {
        test(&mut out, item);
    }
    out
}

fn test(out: &mut String, test: &DraftTest) {
    let mut fields = vec![("id", json!(test.id)), ("name", json!(test.name)), ("call", json!(test.call))];
    if test.no_auth {
        fields.push(("auth", json!("none")));
    }
    if !test.vars.is_empty() {
        fields.push(("vars", json!(test.vars)));
    }
    fields.push(("expect", json!({ "status": test.status })));
    fields_item(out, 2, &fields);
}

pub fn snapshot(file_id: &str, source: &str, doc: &Value) -> FileChange {
    let mut after = format!(
        "# Snapshot of {source} as OpenAPI 3.1, written by `vc import` and `vc sync`.\n\
         # `vc sync {file_id}` merges against it: commit it, and do not edit it.\n"
    );
    body(&mut after, 0, doc);
    FileChange { path: format!("openapi/{file_id}.yaml"), before: None, after, moved_from: None }
}

#[cfg(test)]
#[path = "import_write_tests.rs"]
mod tests;
