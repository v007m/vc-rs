use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use vc_interfaces::{Discovery, SourceError, SourceFilter, SourceProvider};
use vc_model::SourcePath;
use vc_testkit::{FakeVcs, MemorySources};

use super::ImportRequest;
use crate::sample::builder;
use crate::{Engine, EngineError};

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
        let files: [(&str, &[u8]); 2] = [("vibranium.config.yaml", CONFIG), (SPEC, PETSTORE.as_bytes())];
        let sources = Arc::new(Self { files: MemorySources::new(&files), linked: AtomicBool::new(linked) });
        builder(FakeVcs::default()).sources(sources).build().unwrap()
    }
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
