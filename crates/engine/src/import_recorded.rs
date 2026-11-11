//! A recording's draft (HAR, curl) merged into the project. Each origin goes to the api file
//! whose base URL renders to it under the active profiles: an api with the same method and
//! path shape is reused, the rest are appended. An origin no file has becomes a new api file
//! with a base-URL var. The secrets the draft references are declared in `.env.schema`, and the
//! flow test is written with its calls pointed at the apis it reuses.

use indexmap::IndexMap;
use serde_json::json;
use vc_interfaces::{DraftApi, DraftFlow, DraftGroup, DraftStep, ImportDraft};
use vc_model::{DEFAULT_SECRETS_SCHEMA, FileKind, SourceFile};

use crate::EngineError;
use crate::authoring::{Authoring, Opened, refused};
use crate::create::{DOCS, ListItem, block, commented_item, derived_path, header, sentence};
use crate::create_api::{base_var_in, config};
use crate::edit::{Changes, FileChange, declaring, internal};
use crate::import_homes::{Home, path_vars, shape};
use crate::import_titles::{Placed, Written, titles};
use crate::import_write::{api_fields, api_file, new_file};
use crate::rename::item_ids;
use crate::staged::{Edited, Staged};
use crate::yaml_out::{entry, fields_item};

/// The project a recording goes into: its files, where its api files point, and its
/// `.env.schema`, when it has one.
pub struct Recording<'a> {
    pub project: &'a Authoring,
    pub homes: &'a [Home],
    pub schema: Option<&'a SourceFile>,
    /// `--id`, for a draft of one origin: the api file it goes to, whatever file its host
    /// renders to.
    pub file_id: Option<&'a str>,
}

/// Where a draft api went: its full ID in the project, and the names its path vars have there.
#[derive(Default)]
struct Called {
    call: String,
    renames: IndexMap<String, String>,
}

/// The files changed so far, where each draft api went, and what each group became.
#[derive(Default)]
struct Writing {
    staged: Staged,
    calls: IndexMap<String, Called>,
    placed: Vec<Placed>,
}

/// The files `draft`, read from `source`, becomes in `recording`'s project, with a title line
/// for each group, the secrets, `.env.schema` and the flow; each still to pass the guard.
///
/// # Errors
///
/// [`EngineError::Edit`] when a new group's file ID has an api file pointing elsewhere, a
/// config var holds another URL, or the flow's test ID is taken.
pub fn recorded(recording: &Recording<'_>, draft: &ImportDraft, source: &str) -> Result<Changes, EngineError> {
    let mut writing = Writing::default();
    for group in &draft.groups {
        writing.place(recording, group)?;
    }
    let schema = writing.declare(recording.schema, draft);
    let flow = match &draft.flow {
        Some(flow) => Some(writing.flow(recording.project, flow)?),
        None => None,
    };
    let titles = titles(draft, source, &Written { placed: &writing.placed, schema, flow });
    Ok(Changes { files: writing.staged.done(), titles })
}

impl Writing {
    /// Adds `group` to the file its origin renders to, or to a new one.
    fn place(&mut self, recording: &Recording<'_>, group: &DraftGroup) -> Result<(), EngineError> {
        let origin = origin_of(group).to_ascii_lowercase();
        let homes = recording.homes.iter().filter(|home| home.origin == origin);
        let homes = homes.filter(|home| group.apis.iter().all(|api| under(&api.path, &home.prefix).is_some()));
        let homes = homes.filter(|home| recording.file_id.is_none_or(|id| home.file_id == id));
        match homes.max_by_key(|home| home.prefix.len()) {
            Some(home) => self.merge(recording.project, home, group),
            None => self.create(recording.project, group, recording.file_id.unwrap_or(&group.file_id)),
        }
    }

    /// `group`'s apis in `home`'s file: each reused when the file has its method and path
    /// shape, else appended with the base URL's path taken off its own.
    fn merge(&mut self, project: &Authoring, home: &Home, group: &DraftGroup) -> Result<(), EngineError> {
        let found = project.find(FileKind::Api, &home.file_id).ok_or_else(|| internal("an api file not found"))?;
        let mut placed = Placed::of(group, &home.file_id, true);
        placed.auth.clone_from(&home.auth);
        for api in &group.apis {
            let path = under(&api.path, &home.prefix).unwrap_or(&api.path);
            let known = home.apis.iter().find(|known| known.method == api.method && known.shape == shape(path));
            let called = if let Some(known) = known {
                placed.matched += 1;
                let renames = path_vars(&api.path).into_iter().zip(known.vars.iter().cloned());
                let renames = renames.filter(|(from, to)| from != to).collect();
                Called { call: format!("{}#{}", home.file_id, known.id), renames }
            } else {
                placed.added += 1;
                let moved = DraftApi { path: path.to_owned(), ..api.clone() };
                let id = self.staged.edit(found, |opened| appended(opened, &moved))?;
                Called { call: format!("{}#{id}", home.file_id), ..Called::default() }
            };
            self.calls.insert(format!("{}#{}", group.file_id, api.id), called);
        }
        self.placed.push(placed);
        Ok(())
    }

    /// `group` as the new api file `id`, its origin a config var.
    fn create(&mut self, project: &Authoring, group: &DraftGroup, id: &str) -> Result<(), EngineError> {
        if project.find(FileKind::Api, id).is_some() {
            return Err(refused(
                format!("`{id}` already has an api file, whose base URL is not {}", origin_of(group)),
                &format!("pass the `--profile` whose vars point `{id}` there, or `vc rename {id}` first"),
            ));
        }
        let mut base = group.base_url.clone().unwrap_or_default();
        if let Some((draft_var, origin)) = group.config_vars.iter().next() {
            let origin = origin.as_str().unwrap_or_default();
            let var = self.staged.edit(config(project)?, |opened| base_var_in(opened, id, origin))?;
            base = base.replace(&format!("${{var.{draft_var}}}"), &format!("${{var.{var}}}"));
        }
        let named = DraftGroup { file_id: id.to_owned(), ..group.clone() };
        self.staged.keep(new_file(FileKind::Api, id, &api_file(&named, &base, None))?);
        for api in &group.apis {
            let call = format!("{id}#{}", api.id);
            self.calls.insert(format!("{}#{}", group.file_id, api.id), Called { call, ..Called::default() });
        }
        self.placed.push(Placed { added: group.apis.len(), ..Placed::of(group, id, false) });
        Ok(())
    }

    /// The secrets `draft` references that `schema` does not declare, added to it; none when
    /// the project has no schema, and the title then says where to add them.
    fn declare(&mut self, schema: Option<&SourceFile>, draft: &ImportDraft) -> Option<String> {
        let mut names: Vec<&str> = Vec::new();
        for secret in &draft.secrets {
            if !names.contains(&secret.name.as_str()) {
                names.push(&secret.name);
            }
        }
        let Some(schema) = schema else {
            return (!names.is_empty()).then(|| format!("add {} to {DEFAULT_SECRETS_SCHEMA}", names.join("=, ") + "="));
        };
        let (change, added) = declaring(schema, &names)?;
        self.staged.keep(change);
        Some(format!("{DEFAULT_SECRETS_SCHEMA}  declares {}", added.join(", ")))
    }

    /// `flow` as a test in its file, calling the apis its steps went to; the title names it.
    fn flow(&mut self, project: &Authoring, flow: &DraftFlow) -> Result<String, EngineError> {
        let (file, test) = (flow.file_id.as_str(), flow.test_id.as_str());
        let mut yaml = String::new();
        entry(&mut yaml, 0, ("id", &json!(test)));
        entry(&mut yaml, 0, ("name", &json!(sentence(test))));
        yaml.push_str("steps:\n");
        flow.steps.iter().for_each(|step| self.step(&mut yaml, step));
        let comment = format!("# {file}#{test} · replays the recording · keys: {DOCS}#tests");
        let new = ListItem { list: "tests", yaml: &yaml, comment: &comment };
        if let Some(found) = project.find(FileKind::Test, file) {
            self.staged.edit(found, |opened| appended_test(opened, test, &new))?;
        } else {
            let path = derived_path(FileKind::Test, file)?.to_string();
            let after = format!("{}tests:\n  {comment}\n{}", header(FileKind::Test, file), block("  ", &yaml));
            self.staged.keep(FileChange { path, before: None, after, moved_from: None });
        }
        Ok(format!("flow {file}#{test} ({} steps)", flow.steps.len()))
    }

    /// `step` as an item of `steps`, calling where its api went, its vars by their names there.
    fn step(&self, out: &mut String, step: &DraftStep) {
        let called = self.calls.get(&step.call);
        let renamed = |name: &String| called.and_then(|called| called.renames.get(name)).unwrap_or(name).clone();
        let mut fields = vec![("call", json!(called.map_or(&step.call, |called| &called.call)))];
        if !step.vars.is_empty() {
            let vars: serde_json::Map<_, _> =
                step.vars.iter().map(|(name, value)| (renamed(name), value.clone())).collect();
            fields.push(("vars", serde_json::Value::Object(vars)));
        }
        if !step.save.is_empty() {
            fields.push(("save", json!(step.save)));
        }
        fields_item(out, 2, &fields);
    }
}

/// `api` appended to the api file `opened`, under an ID it does not have yet; that ID.
fn appended(opened: &Opened<'_>, api: &DraftApi) -> Result<Edited<String>, EngineError> {
    let taken = item_ids(opened.document.root());
    let ids = (1..=taken.len() + 1).map(|n| if n == 1 { api.id.clone() } else { format!("{}{n}", api.id) });
    let id = ids.into_iter().find(|id| !taken.contains(&id.as_str())).unwrap_or_else(|| api.id.clone());
    let mut yaml = String::new();
    for (key, value) in api_fields(&DraftApi { id: id.clone(), ..api.clone() }) {
        entry(&mut yaml, 0, (key, &value));
    }
    let file = FileKind::Api.file_id(opened.file.path.as_str());
    let comment = format!("# {file}#{id} · {} {} · recorded · keys: {DOCS}#api-files", api.method.name(), api.path);
    let (change, _) = commented_item(opened, &ListItem { list: "apis", yaml: &yaml, comment: &comment })?;
    Ok((Some(change), id))
}

/// `new`, the test `test`, appended to the test file `opened`, which must not have it yet.
fn appended_test(opened: &Opened<'_>, test: &str, new: &ListItem<'_>) -> Result<Edited<()>, EngineError> {
    if item_ids(opened.document.root()).contains(&test) {
        let full = format!("{}#{test}", FileKind::Test.file_id(opened.file.path.as_str()));
        return Err(refused(format!("`{full}` already exists"), "give `--flow` another test ID"));
    }
    let (change, _) = commented_item(opened, new)?;
    Ok((Some(change), ()))
}

/// The origin a draft group was recorded at: its base-URL var's value.
fn origin_of(group: &DraftGroup) -> &str {
    group.config_vars.values().next().and_then(serde_json::Value::as_str).unwrap_or_default()
}

/// `path` with `prefix` taken off, when it is under it: `/v1/carts` under `/v1` is `/carts`.
fn under<'p>(path: &'p str, prefix: &str) -> Option<&'p str> {
    let rest = path.strip_prefix(prefix)?;
    match rest {
        "" => Some("/"),
        _ if rest.starts_with('/') => Some(rest),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::under;

    #[test]
    fn a_path_is_under_a_base_path_only_at_a_segment_boundary() {
        assert_eq!(under("/v1/carts", "/v1"), Some("/carts"));
        assert_eq!(under("/v1", "/v1"), Some("/"));
        assert_eq!(under("/v12/carts", "/v1"), None);
        assert_eq!(under("/carts", ""), Some("/carts"));
    }
}
