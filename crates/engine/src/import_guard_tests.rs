use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use vc_interfaces::{Discovery, SourceError, SourceFilter, SourceProvider};
use vc_model::SourcePath;
use vc_testkit::{FakeVcs, MemorySources};

use super::ImportRequest;
use crate::sample::builder;
use crate::{Engine, EngineError, SyncRequest};

const PETSTORE: &str = include_str!("../../adapters/importers/fixtures/petstore-3.1.json");
const SHOP: &str = include_str!("../../adapters/importers/fixtures/shop.curl");
const CONFIG: &[u8] = b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n";
const SPEC: &str = "specs/users.yaml";

/// A project in memory whose `openapi/users.yaml` is a link to `specs/users.yaml`: from the
/// start, or only once the spec has been read, as if it were swapped in while it was fetched.
struct Linked {
    files: MemorySources,
    linked: AtomicBool,
}

impl Linked {
    fn engine(linked: bool) -> Engine {
        Self::with(linked, &[])
    }

    /// The project, with `more` files in it.
    fn with(linked: bool, more: &[(&str, &[u8])]) -> Engine {
        let mut files: Vec<(&str, &[u8])> = vec![("vibranium.config.yaml", CONFIG), (SPEC, PETSTORE.as_bytes())];
        files.extend_from_slice(more);
        let sources = Arc::new(Self { files: MemorySources::new(&files), linked: AtomicBool::new(linked) });
        builder(FakeVcs::default()).sources(sources).build().unwrap()
    }
}

/// The project, with `more` files in it and no link anywhere.
fn unlinked(more: &[(&str, &[u8])]) -> Engine {
    let mut files: Vec<(&str, &[u8])> = vec![("vibranium.config.yaml", CONFIG), (SPEC, PETSTORE.as_bytes())];
    files.extend_from_slice(more);
    builder(FakeVcs::default()).sources(Arc::new(MemorySources::new(&files))).build().unwrap()
}

impl SourceProvider for Linked {
    fn discover(&self, filter: &SourceFilter) -> Result<Discovery, SourceError> {
        self.files.discover(filter)
    }

    fn read(&self, paths: &[SourcePath]) -> Result<Discovery, SourceError> {
        if paths.iter().any(|path| path.as_str() == SPEC) {
            self.linked.store(true, Ordering::SeqCst);
        }
        self.files.read(paths)
    }

    fn read_bytes(&self, path: &SourcePath) -> Result<Vec<u8>, SourceError> {
        self.files.read_bytes(path)
    }

    fn real(&self, path: &SourcePath) -> Option<SourcePath> {
        let linked = path.as_str() == "openapi/users.yaml" && self.linked.load(Ordering::SeqCst);
        let target = if linked { SourcePath::new(SPEC).ok()? } else { path.clone() };
        self.files.read_bytes(&target).ok().map(|_| target)
    }
}

#[tokio::test]
async fn a_snapshot_path_made_a_link_while_the_spec_is_fetched_is_refused_before_the_write() {
    let spec = ImportRequest { kind: "openapi".to_owned(), source: SPEC.to_owned(), ..ImportRequest::default() };
    let Err(EngineError::Edit { reason, help }) = Linked::engine(false).import(&spec).await else {
        panic!("the guard runs again after the fetch")
    };
    assert_eq!(reason, "openapi/users.yaml, where `vc import` keeps the snapshot, leads to specs/users.yaml");
    let plain = "make `openapi/users.yaml` a plain file of its own, or use another file ID";
    assert_eq!(help.as_deref(), Some(plain));
}

#[tokio::test]
async fn a_recording_keeps_no_snapshot_so_a_link_at_that_path_does_not_refuse_it() {
    let curl = ImportRequest {
        kind: "curl".to_owned(),
        source: "rec/shop.curl".to_owned(),
        text: Some(SHOP.to_owned()),
        file_id: Some("users".to_owned()),
        ..ImportRequest::default()
    };
    let changes = Linked::engine(true).import(&curl).await.unwrap();
    let paths: Vec<_> = changes.files.iter().map(|file| file.path.as_str()).collect();
    assert!(paths.contains(&"services/users/users.api.yaml"), "{paths:?}");
    assert!(paths.iter().all(|path| !path.starts_with("openapi/")), "{paths:?}");
}

/// The api file `users`, synced from [`SPEC`].
const USERS_API: &[u8] =
    b"baseUrl: http://127.0.0.1:9\nsource:\n  openapi: specs/users.yaml\napis:\n  - id: listPets\n    path: /pets\n";

#[tokio::test]
async fn sync_checks_the_snapshot_path_again_once_the_specs_are_fetched() {
    let engine = Linked::with(false, &[("services/users/users.api.yaml", USERS_API)]);
    let users = SyncRequest { files: vec!["users".to_owned()], ..SyncRequest::default() };
    let Err(EngineError::Edit { reason, .. }) = engine.sync(&users).await else {
        panic!("the guard runs again after the fetches")
    };
    assert_eq!(reason, "openapi/users.yaml, where `vc import` keeps the snapshot, leads to specs/users.yaml");
}

#[tokio::test]
async fn a_snapshot_path_another_api_file_syncs_from_is_refused() {
    let pay: &[u8] = b"baseUrl: http://127.0.0.1:9\nsource:\n  openapi: openapi/users.yaml\napis: []\n";
    let engine = Linked::with(false, &[("services/pay/pay.api.yaml", pay)]);
    let users = ImportRequest { kind: "openapi".to_owned(), source: SPEC.to_owned(), ..ImportRequest::default() };
    let Err(EngineError::Edit { reason, help }) = engine.import(&users).await else {
        panic!("writing the snapshot would overwrite the spec `pay` syncs from")
    };
    let spec = "openapi/users.yaml, where `vc import` keeps the snapshot of `users`, is the spec `pay` syncs from";
    assert_eq!(reason, spec);
    let moved = "point `source.openapi` in `pay` at a spec outside `openapi/`, or use another file ID";
    assert_eq!(help.as_deref(), Some(moved));
}

#[tokio::test]
async fn an_import_over_a_kept_snapshot_diffs_from_what_is_there() {
    let kept: &[u8] = b"# kept by an earlier import\nopenapi: 3.1.0\n";
    let engine = unlinked(&[("openapi/users.yaml", kept)]);
    let users = ImportRequest { kind: "openapi".to_owned(), source: SPEC.to_owned(), ..ImportRequest::default() };
    let changes = engine.import(&users).await.unwrap();
    let snapshot = changes.files.iter().find(|file| file.path == "openapi/users.yaml").unwrap();
    assert_eq!(snapshot.before.as_deref(), Some("# kept by an earlier import\nopenapi: 3.1.0\n"));
    assert!(snapshot.diff().contains("--- a/openapi/users.yaml\n"), "{}", snapshot.diff());
}
