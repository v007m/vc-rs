use std::path::PathBuf;
use std::sync::Arc;

use vc_compiler::CompileCache;
use vc_diagnostics::Code;
use vc_interfaces::{CompileCacheError, CompileCacheStore, SourceProvider};
use vc_testkit::{FixedClock, MemoryCompileCache, MemorySources};

use crate::{CheckRequest, Engine, EngineBuilder, EngineError};

const CONFIG: (&str, &[u8]) =
    ("vibranium.config.yaml", b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n");
const USERS: (&str, &[u8]) = ("services/users/users.api.yaml", b"apis: []\n");
const ORDERS: (&str, &[u8]) = ("services/orders/orders.api.yaml", b"apis: []\n");
const GET_USER: (&str, &[u8]) = ("services/users/users.api.yaml", b"apis:\n  - id: getUser\n    path: /users\n");

fn engine(files: &[(&str, &[u8])], cache: Arc<dyn CompileCacheStore>) -> Engine {
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(files));
    EngineBuilder::new(PathBuf::from("/project")).sources(sources).compile_cache(cache).build().unwrap()
}

fn request(paths: &[&str]) -> CheckRequest {
    CheckRequest { paths: paths.iter().map(|&path| path.to_owned()).collect() }
}

/// A store whose every call fails.
struct BrokenCache;

impl CompileCacheStore for BrokenCache {
    fn load(&self) -> Result<Option<Vec<u8>>, CompileCacheError> {
        Err(CompileCacheError::Read { location: "broken".to_owned(), reason: "no disk".to_owned() })
    }

    fn save(&self, _bytes: &[u8]) -> Result<(), CompileCacheError> {
        Err(CompileCacheError::Write { location: "broken".to_owned(), reason: "no disk".to_owned() })
    }
}

#[test]
fn checks_a_clean_project_and_leaves_a_cache() {
    let cache = MemoryCompileCache::new();
    let report = engine(&[CONFIG, USERS], Arc::new(cache.clone())).check(&CheckRequest::default()).unwrap();
    assert!(report.diagnostics.is_empty() && report.notices.is_empty());
    assert_eq!((report.checked, report.files.len()), (2, 2));
    assert!(cache.load().unwrap().is_some_and(|blob| !blob.is_empty()), "the next check gets a cache");
}

#[test]
fn a_check_from_the_saved_blob_reuses_every_unit() {
    let cold = MemoryCompileCache::new();
    let first = engine(&[CONFIG, GET_USER], Arc::new(cold.clone())).check(&CheckRequest::default()).unwrap();
    let blob = cold.load().unwrap().unwrap();
    assert_eq!(CompileCache::from_bytes(&blob).units(), 2, "the blob holds every file's unit");
    let warm = MemoryCompileCache::holding(Some(blob));
    let second = engine(&[CONFIG, GET_USER], Arc::new(warm.clone())).check(&CheckRequest::default()).unwrap();
    assert_eq!(second, first);
    assert!(second.notices.is_empty(), "{:?}", second.notices);
    let saved = warm.load().unwrap().unwrap();
    assert_eq!(CompileCache::from_bytes(&saved).units(), 2);
}

/// A store holding a blob that refuses every save.
struct ReadOnlyCache(Vec<u8>);

impl CompileCacheStore for ReadOnlyCache {
    fn load(&self) -> Result<Option<Vec<u8>>, CompileCacheError> {
        Ok(Some(self.0.clone()))
    }

    fn save(&self, _bytes: &[u8]) -> Result<(), CompileCacheError> {
        Err(CompileCacheError::Write { location: "read-only".to_owned(), reason: "no disk".to_owned() })
    }
}

#[test]
fn saves_the_cache_only_when_the_files_changed() {
    let cold = MemoryCompileCache::new();
    engine(&[CONFIG, GET_USER], Arc::new(cold.clone())).check(&CheckRequest::default()).unwrap();
    let blob = cold.load().unwrap().unwrap();
    let same = engine(&[CONFIG, GET_USER], Arc::new(ReadOnlyCache(blob.clone()))).check(&CheckRequest::default());
    assert!(same.unwrap().notices.is_empty(), "nothing changed, so nothing is saved");
    let changed = engine(&[CONFIG, USERS], Arc::new(ReadOnlyCache(blob))).check(&CheckRequest::default());
    assert!(changed.unwrap().notices[0].contains("not saved"));
}

#[test]
fn reports_unreadable_files_as_v1012_errors() {
    let report = engine(&[CONFIG, ("bad.api.yaml", &[0xff]), USERS], Arc::new(MemoryCompileCache::new()))
        .check(&CheckRequest::default())
        .unwrap();
    assert!(report.has_errors());
    let found: Vec<_> = report.diagnostics.iter().map(|d| (d.code, report.files.get(d.primary.span.file))).collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, Code::V1012);
    assert_eq!(found[0].1.map(|file| file.path.as_str()), Some("bad.api.yaml"));
    assert_eq!(report.checked, 3, "the unreadable file still counts as checked");
}

#[test]
fn a_cache_that_fails_only_adds_notices() {
    let report = engine(&[CONFIG], Arc::new(BrokenCache)).check(&CheckRequest::default()).unwrap();
    assert!(report.diagnostics.is_empty());
    assert_eq!(report.notices.len(), 2, "{:?}", report.notices);
    assert!(report.notices[0].contains("not used") && report.notices[1].contains("not saved"));
}

#[test]
fn reports_only_on_the_requested_paths() {
    let files: [(&str, &[u8]); 5] =
        [CONFIG, USERS, ORDERS, ("services/users/bad.test.yaml", &[0xff]), ("services/orders/bad.test.yaml", &[0xff])];
    let engine = engine(&files, Arc::new(MemoryCompileCache::new()));
    let report = engine.check(&request(&["./services/users/"])).unwrap();
    assert_eq!(report.checked, 2);
    let paths: Vec<_> = report
        .diagnostics
        .iter()
        .filter_map(|d| report.files.get(d.primary.span.file))
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(paths, ["services/users/bad.test.yaml"]);
    assert_eq!(engine.check(&request(&["."])).unwrap().checked, 5);
}

#[test]
fn a_path_that_selects_nothing_is_an_error() {
    let engine = engine(&[CONFIG, USERS], Arc::new(MemoryCompileCache::new()));
    let error = engine.check(&request(&["services/users", "services/user"])).unwrap_err();
    assert_eq!(error, EngineError::NoFilesMatch { path: "services/user".to_owned() });
}

/// The codes and helps a check finds on 2026-11-03 in a users service holding `api`, a test
/// skipped until 2026-11-02, and `data/user.json`.
fn checked_on_a_fixed_day(api: &[u8]) -> Vec<(Code, Vec<String>)> {
    let crud: &[u8] =
        b"tests:\n  - id: makesUser\n    call: users#make\n    skip: {reason: flaky, until: 2026-11-02}\n";
    let files = [
        CONFIG,
        ("services/users/users.api.yaml", api),
        ("services/users/crud.test.yaml", crud),
        ("services/users/data/user.json", b"{}"),
    ];
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(&files));
    let today = FixedClock(vc_model::Date::parse("2026-11-03").unwrap());
    let builder = EngineBuilder::new(PathBuf::from("/project")).sources(sources).clock(Arc::new(today));
    let engine = builder.compile_cache(Arc::new(MemoryCompileCache::new())).build().unwrap();
    let report = engine.check(&CheckRequest::default()).unwrap();
    report.diagnostics.iter().map(|found| (found.code, found.help.clone())).collect()
}

#[test]
fn passes_today_to_the_compiler() {
    let api = b"apis:\n  - id: make\n    method: POST\n    path: /users\n    body: {$file: data/user.json}\n";
    let found = checked_on_a_fixed_day(api);
    assert_eq!(found.iter().map(|(code, _)| *code).collect::<Vec<_>>(), [Code::V3104]);
}

#[test]
fn passes_the_other_files_to_the_compiler() {
    let api = b"apis:\n  - id: make\n    method: POST\n    path: /users\n    body: {$file: data/usr.json}\n";
    let found = checked_on_a_fixed_day(api);
    assert_eq!(found, [(Code::V2004, vec!["did you mean `data/user.json`?".to_owned()])]);
}

#[test]
fn finds_no_project_without_a_config() {
    let missing = std::env::temp_dir().join("vc-engine-tests-no-such-folder");
    assert!(matches!(EngineBuilder::for_project(&missing), Err(EngineError::NoProject { .. })));
}
