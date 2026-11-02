//! A generated project uses most of the syntax reference and is clean, so `vc check` reports
//! nothing on it; the compile benches rely on that.

use std::path::PathBuf;
use std::sync::Arc;

use vc_engine::{CheckRequest, EngineBuilder};
use vc_testkit::{MemoryCompileCache, generate_project};

#[test]
fn a_generated_project_checks_without_diagnostics() {
    let project = generate_project(11, 60);
    let engine = EngineBuilder::new(PathBuf::from("/generated"))
        .sources(Arc::new(project.sources()))
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .build()
        .unwrap();
    let report = engine.check(&CheckRequest::default()).unwrap();
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(report.checked, 60);
}
