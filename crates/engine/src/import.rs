//! `vc import <kind>`: definition files from a foreign file.

use std::sync::Arc;

use vc_adapter_importers::ImporterRegistry;
use vc_adapter_system::SystemEnvironment;
use vc_interfaces::{ImportOptions, ImportSource, RecordingFilter, SpecFetcher as _};

use crate::authoring::refused;
use crate::fetch::ProjectFetcher;
use crate::import_homes::Home;
use crate::import_recorded::{Recording, recorded};
use crate::{Changes, Engine, EngineError, import_write};

/// What `vc import <kind> <url-or-file>` reads and what it makes of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct ImportRequest {
    /// The importer: `openapi`, `har`, `postman`, `bruno`, `http`, `curl`.
    pub kind: String,
    /// The URL or path, as given; a path is relative to the working folder.
    pub source: String,
    /// `--id`: the file ID of the api file to fill; derived from the spec when not given.
    pub file_id: Option<String>,
    /// `--tests`: a happy-path test per api.
    pub tests: bool,
    /// `--negative`: tests that send what the schema rejects.
    pub negative: bool,
    /// `--auth`: a test per secured api that expects 401 with no auth.
    pub auth: bool,
    /// `--include`: only operations with these tags or path prefixes.
    pub include: Vec<String>,
    /// `--only`: only these operation IDs.
    pub only: Vec<String>,
    /// `--profile`, in order; the profile whose vars render `source.headers`, and, for a
    /// recording, the base URLs its hosts are matched against.
    pub profiles: Vec<String>,
    /// `--flow <[fileId#]testId>`: a test replaying the recording.
    pub flow: Option<String>,
    /// What a recording keeps that its filter would drop.
    pub keep: Keep,
}

/// What a recording keeps that its filter would drop.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct Keep {
    /// `--include-host`: hosts kept though they are third-party.
    pub hosts: Vec<String>,
    /// `--keep-static`: static assets (scripts, styles, images, fonts) are kept.
    pub assets: bool,
}

impl Engine {
    /// The files `request` would create or change: the api file, the spec snapshot
    /// `openapi/<fileId>.yaml`, the config's base-URL var and the generated tests, each passed
    /// through the compile guard (D107).
    ///
    /// The spec is read once; the snapshot is not a definition file, so the guard does not
    /// compile it. What the importer could not carry over is in the titles, `at: message`.
    ///
    /// A recording (a draft with no spec snapshot: HAR, curl) is merged instead: each host goes
    /// to the api file whose base URL renders to it under `request.profiles`, reusing its apis,
    /// and the secrets it references are declared in `.env.schema`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when the spec cannot be fetched or read, the kind is unknown, or a
    /// file to create exists; [`EngineError::Invalid`] when the result would not compile.
    pub async fn import(&self, request: &ImportRequest) -> Result<Changes, EngineError> {
        let project = self.authoring()?;
        let fetcher = self.fetcher()?;
        let bytes = fetcher.fetch(&request.source).await.map_err(|error| refused(error.to_string(), FETCH_HELP))?;
        let importer = ImporterRegistry::builtin().create(&request.kind, fetcher).map_err(|error| {
            let kinds: Vec<_> = ImporterRegistry::builtin().names().map(str::to_owned).collect();
            refused(error.to_string(), &format!("the importers are {}", kinds.join(", ")))
        })?;
        let source = ImportSource { location: request.source.clone(), bytes };
        let homes = self.homes(&request.profiles);
        let mut opts = options(request);
        opts.recording.hosts = hosts(&homes);
        let draft = importer.read(&source, &opts).await.map_err(|error| refused(error.to_string(), READ_HELP))?;
        if draft.snapshot.is_none() {
            let schema = self.secrets_schema();
            let recording = Recording { project: &project, homes: &homes, schema: schema.as_ref() };
            return self.guarded(&project, recorded(&recording, &draft, &request.source)?);
        }
        let (changes, snapshots) = import_write::drafted(&project, &draft, request)?;
        let mut guarded = self.guarded(&project, changes)?;
        guarded.files.extend(snapshots);
        Ok(guarded)
    }

    /// Fetches specs through the project's sources and the run's transport.
    pub(crate) fn fetcher(&self) -> Result<Arc<ProjectFetcher>, EngineError> {
        let environment = self.environment.clone().unwrap_or_else(|| Arc::new(SystemEnvironment::capture()));
        let transport = self.transport(environment.as_ref())?;
        Ok(Arc::new(ProjectFetcher { sources: Arc::clone(&self.sources), transport }))
    }
}

const FETCH_HELP: &str = "give a URL, or a path inside the project";
const READ_HELP: &str = "fix the spec, or import part of it with `--include` or `--only`";

/// What the importer is asked for; with no `--id`, the file ID is the spec's file name up to
/// its first dot (`specs/users.v2.yaml` gives `users`).
fn options(request: &ImportRequest) -> ImportOptions {
    let name = request.source.rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = name.split(['.', '?', '#']).next().unwrap_or_default();
    ImportOptions {
        file_id: request.file_id.clone().unwrap_or_else(|| vc_compiler::file_id(stem)),
        tests: request.tests,
        negative: request.negative,
        auth: request.auth,
        include: request.include.clone(),
        only: request.only.clone(),
        flow: request.flow.clone(),
        recording: RecordingFilter {
            include_hosts: request.keep.hosts.clone(),
            keep_static: request.keep.assets,
            ..RecordingFilter::default()
        },
    }
}

/// The hosts the project's api files point at, without their ports, each once: what a
/// recording keeps as first-party.
fn hosts(homes: &[Home]) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    for home in homes {
        let authority = home.origin.split_once("://").map_or(home.origin.as_str(), |(_, rest)| rest);
        let host = authority.rsplit_once(':').map_or(authority, |(host, _)| host);
        if !hosts.iter().any(|known| known == host) {
            hosts.push(host.to_owned());
        }
    }
    hosts
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
