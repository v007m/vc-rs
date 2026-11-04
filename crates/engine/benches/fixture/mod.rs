//! The 1,000-file project the compile benches check, and engines over it with an empty, a full
//! or a nearly full compile cache (D119, D120).

use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Arc;

use vc_engine::{CheckRequest, Engine, EngineBuilder};
use vc_interfaces::CompileCacheStore;
use vc_testkit::{GeneratedProject, MemoryCompileCache};

/// How many files the project has; the budgets are stated for this size.
const FILES: usize = 1_000;
/// Fixed, so every run checks the same bytes.
const SEED: u64 = 2_026;

/// Which compile a bench measures.
#[derive(Clone, Copy, Debug)]
pub enum Scenario {
    /// No compile cache: every file is compiled.
    Cold,
    /// The cache from a check of the same files: nothing changed.
    Warm,
    /// The cache from a check before the last file was edited.
    Changed,
}

/// The project, its one-file-edited twin, and the cache a check of the project leaves.
pub struct Bench {
    project: GeneratedProject,
    changed: GeneratedProject,
    cache: Option<Vec<u8>>,
}

impl Bench {
    /// Generates the project and checks it once to fill the cache.
    pub fn prepare() -> Self {
        let project = vc_testkit::generate_project(SEED, FILES);
        let changed = project.with_one_file_changed();
        let store = MemoryCompileCache::new();
        check(&build(&project, Arc::new(store.clone())));
        let cache = or_exit(store.load());
        Self { project, changed, cache }
    }

    /// A fresh engine for `scenario`, its cache store holding what that scenario starts from.
    pub fn engine(&self, scenario: Scenario) -> Engine {
        let (project, cache) = match scenario {
            Scenario::Cold => (&self.project, None),
            Scenario::Warm => (&self.project, self.cache.clone()),
            Scenario::Changed => (&self.changed, self.cache.clone()),
        };
        build(project, Arc::new(MemoryCompileCache::holding(cache)))
    }
}

/// Runs `vc check` on the whole project and returns how many diagnostics it reported.
pub fn check(engine: &Engine) -> usize {
    or_exit(engine.check(&CheckRequest::default())).diagnostics.iter().count()
}

fn build(project: &GeneratedProject, cache: Arc<dyn CompileCacheStore>) -> Engine {
    let builder = EngineBuilder::new(PathBuf::from("/bench")).sources(Arc::new(project.sources()));
    or_exit(builder.compile_cache(cache).build())
}

/// A bench cannot measure anything once its setup failed, so it says why and stops.
fn or_exit<T, E: Display>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| {
        eprintln!("bench setup failed: {error}");
        std::process::exit(1)
    })
}
