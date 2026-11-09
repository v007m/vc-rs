//! `vc sync [fileId...]`: each api file with a `source` merged three ways, its snapshot
//! `openapi/<fileId>.yaml` the base, the spec now theirs and the file mine. The rules are in
//! `sync_rules.rs`, the plan in `sync_plan.rs`, the edits in `sync_apply.rs`.

use std::sync::Arc;

use indexmap::IndexMap;
use serde_json::Value;
use vc_adapter_importers::ImporterRegistry;
use vc_interfaces::{DraftApi, ImportOptions, ImportSource, Importer, SpecFetcher};
use vc_model::{FileKind, SourceFile, SourceFileId};
use vc_parser::Node;

use crate::authoring::{Authoring, Named, Opened, refused};
use crate::edit::{FileChange, internal};
use crate::fetch::ProjectFetcher;
use crate::sync_plan::{Mine, SyncSide, Versions, plan};
use crate::sync_snapshot::kept;
use crate::{Changes, Engine, EngineError, import_write, sync_apply};

/// What `vc sync` does with what it finds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(clippy::exhaustive_enums, reason = "closed set of `vc sync` modes (D160)")]
pub enum SyncMode {
    /// Merges and returns the changes to write.
    #[default]
    Write,
    /// `--dry-run`: the same changes, shown and not written.
    DryRun,
    /// `--check`: writes nothing; drift is exit 4 (D127).
    Check,
}

/// What `vc sync [fileId...]` merges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct SyncRequest {
    /// The api files to sync; every file with a `source` when empty.
    pub files: Vec<String>,
    /// Write, preview or check.
    pub mode: SyncMode,
    /// `--take spec|mine <apiId>` and `--resolved <apiId>`: conflicts settled, by api ID.
    pub settle: Vec<(String, SyncSide)>,
}

/// What a sync found and would write.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SyncReport {
    /// The api files and snapshots to write; a line per change in the titles.
    pub changes: Changes,
    /// Whether any spec changed since its snapshot.
    pub drift: bool,
    /// The apis left in conflict.
    pub conflicts: usize,
}

impl Engine {
    /// Fetches each selected file's spec and merges it into the api file against the snapshot.
    /// The api files pass the compile guard (D107); the snapshots are not definition files.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for an unknown file, one with no `source`, a spec that cannot be
    /// fetched or read, or a change that cannot be made in the file as written;
    /// [`EngineError::Invalid`] when the result would not compile.
    pub async fn sync(&self, request: &SyncRequest) -> Result<SyncReport, EngineError> {
        let project = self.authoring()?;
        let syncing = Syncing { project: &project, fetcher: self.fetcher()?, request };
        let mut report = SyncReport::default();
        let mut snapshots = Vec::new();
        for file_id in selected(&project, &request.files)? {
            let synced = syncing.file(&file_id).await?;
            report.drift |= synced.drift;
            report.conflicts += synced.conflicts;
            report.changes.titles.extend(synced.lines);
            report.changes.files.extend(synced.api);
            snapshots.extend(synced.snapshot);
        }
        report.changes = self.guarded(&project, report.changes)?;
        report.changes.files.extend(snapshots);
        Ok(report)
    }
}

const READ_HELP: &str = "fix the spec, or narrow it with `source.include`";

/// The api files named, or every one with a `source`.
fn selected(project: &Authoring, files: &[String]) -> Result<Vec<String>, EngineError> {
    if files.is_empty() {
        let apis = project.entries().filter(|(_, file)| file.kind() == Some(FileKind::Api));
        let with_source = apis.filter(|&found| sourced(found));
        return Ok(with_source.map(|(_, file)| FileKind::Api.file_id(file.path.as_str())).collect());
    }
    for file_id in files {
        if project.find(FileKind::Api, file_id).is_none() {
            return Err(Authoring::unknown("api file", file_id, &project.known(FileKind::Api, "apis")));
        }
    }
    Ok(files.to_vec())
}

/// Whether the api file has a `source` at its root; one that does not parse has none.
fn sourced(found: (SourceFileId, &SourceFile)) -> bool {
    Authoring::open(found).is_ok_and(|opened| opened.document.root().get("source").is_some())
}

struct Syncing<'p> {
    project: &'p Authoring,
    fetcher: Arc<ProjectFetcher>,
    request: &'p SyncRequest,
}

/// What syncing one api file came to.
struct Synced {
    api: Option<FileChange>,
    snapshot: Option<FileChange>,
    lines: Vec<String>,
    drift: bool,
    conflicts: usize,
}

/// A spec read: its apis as the importer drafts them, its bundled document and its text.
struct Spec {
    apis: Vec<DraftApi>,
    doc: Value,
    text: String,
}

/// The three versions of one api file as read.
struct Read<'p> {
    file_id: &'p str,
    opened: Opened<'p>,
    location: String,
    theirs: Spec,
    base: Option<Spec>,
}

impl<'p> Syncing<'p> {
    async fn file(&self, file_id: &'p str) -> Result<Synced, EngineError> {
        let found = self.project.find(FileKind::Api, file_id).ok_or_else(|| internal("the file went away"))?;
        let opened = Authoring::open(found)?;
        let (location, include) = source(opened.document.root(), file_id)?;
        let opts = ImportOptions { file_id: file_id.to_owned(), include, ..ImportOptions::default() };
        let fetcher: Arc<dyn SpecFetcher> = Arc::clone(&self.fetcher) as _;
        let reader =
            Reader { importer: ImporterRegistry::builtin().create("openapi", fetcher).map_err(internal)?, opts };
        let bytes = self.fetcher.fetch(&location).await.map_err(|error| refused(error.to_string(), FETCH_HELP))?;
        let theirs = reader.read(&location, bytes).await?;
        let snapshot_path = format!("openapi/{file_id}.yaml");
        let base = match self.fetcher.fetch(&snapshot_path).await {
            Ok(bytes) => Some(reader.read(&snapshot_path, bytes).await?),
            Err(_) => None,
        };
        self.merged(&Read { file_id, opened, location, theirs, base })
    }

    fn merged(&self, read: &Read<'_>) -> Result<Synced, EngineError> {
        let mine = sync_apply::mine(read.opened.document.root());
        let settled = settled(&self.request.settle, read.file_id, &mine)?;
        let base = read.base.as_ref();
        let versions = Versions {
            file_id: read.file_id,
            base: base.map(|base| base.apis.as_slice()).unwrap_or_default(),
            theirs: &read.theirs.apis,
            mine: &mine,
            settled: &settled,
        };
        let plan = plan(&versions);
        let after = sync_apply::apply(read.opened.document.clone(), &plan.steps)?;
        let file = read.opened.file;
        let api = (after.text() != file.text.as_str()).then(|| FileChange {
            path: file.path.to_string(),
            before: Some(file.text.clone()),
            after: after.text().to_owned(),
            moved_from: None,
        });
        let doc = kept(&read.theirs.doc, base.map(|base| &base.doc), &plan.conflicted);
        let mut snapshot = import_write::snapshot(read.file_id, &read.location, &doc);
        snapshot.before = base.map(|base| base.text.clone());
        let mut lines = vec![format!("fetched {} ({} operations)", read.location, read.theirs.apis.len())];
        lines.extend(plan.lines.iter().map(|line| format!("  {line}")));
        let drift = base.is_none_or(|base| base.doc != read.theirs.doc);
        let snapshot = (snapshot.before.as_deref() != Some(snapshot.after.as_str())).then_some(snapshot);
        Ok(Synced { api, snapshot, lines, drift, conflicts: plan.conflicted.len() })
    }
}

const FETCH_HELP: &str = "check `source.openapi` in the api file: a URL, or a path inside the project";

/// `source.openapi` and `source.include` of the api file `file_id`.
fn source(root: &Node, file_id: &str) -> Result<(String, Vec<String>), EngineError> {
    let source = root.get("source");
    let location = source.and_then(|source| source.get("openapi")).and_then(Node::as_str).ok_or_else(|| {
        refused(format!("`{file_id}` has no `source.openapi`"), "add one, or import the spec with `vc import openapi`")
    })?;
    let include = source.and_then(|source| source.get("include")).map(Node::items).unwrap_or_default();
    Ok((location.to_owned(), include.iter().filter_map(Node::as_str).map(str::to_owned).collect()))
}

/// The conflicts settled in `file_id`, by api ID (`users#update` or `update`).
fn settled(
    settle: &[(String, SyncSide)],
    file_id: &str,
    mine: &[Mine],
) -> Result<IndexMap<String, SyncSide>, EngineError> {
    let mut settled = IndexMap::new();
    for (named, side) in settle {
        let parsed = Named::parse(named);
        let (file, id) = parsed.item.map_or((file_id, parsed.file), |item| (parsed.file, item));
        if file != file_id {
            continue;
        }
        if !mine.iter().any(|api| api.id == id) {
            let known: Vec<String> = mine.iter().map(|api| format!("{file_id}#{}", api.id)).collect();
            return Err(Authoring::unknown("api", named, &known));
        }
        settled.insert(id.to_owned(), *side);
    }
    Ok(settled)
}

struct Reader {
    importer: Box<dyn Importer>,
    opts: ImportOptions,
}

impl Reader {
    async fn read(&self, location: &str, bytes: Vec<u8>) -> Result<Spec, EngineError> {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let source = ImportSource { location: location.to_owned(), bytes };
        let draft =
            self.importer.read(&source, &self.opts).await.map_err(|error| refused(error.to_string(), READ_HELP))?;
        let apis = draft.groups.into_iter().next().map(|group| group.apis).unwrap_or_default();
        Ok(Spec { apis, doc: draft.snapshot.unwrap_or(Value::Null), text })
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
