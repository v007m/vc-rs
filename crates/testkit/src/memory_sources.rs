//! The source contract over [`vc_interfaces::MemorySources`], the in-memory project the tests
//! and the docs playground use; it lives in `vc-interfaces` so the playground can ship it.

use vc_interfaces::{MemorySources, SourceError, SourceFilter, SourceProvider};

use crate::source_provider_suite;

#[test]
fn passes_the_source_provider_suite() {
    source_provider_suite(&mut |files| Box::new(MemorySources::new(files)));
}

#[test]
fn reports_names_a_project_cannot_use() {
    let sources = MemorySources::new(&[("k:v.api.yaml", b""), ("ok.api.yaml", b"")]);
    let discovery = sources.discover(&SourceFilter::default()).unwrap();
    assert_eq!(discovery.files.len(), 1);
    assert_eq!(discovery.problems, [SourceError::InvalidPath { path: "k\u{fffd}v.api.yaml".to_owned() }]);
}
