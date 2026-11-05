use std::path::PathBuf;
use std::sync::Arc;

use vc_diagnostics::Code;
use vc_interfaces::SourceProvider;
use vc_testkit::{MemoryCompileCache, MemorySources};

use crate::{CheckRequest, Engine, EngineBuilder, EngineError};

const CONFIG: (&str, &[u8]) =
    ("vibranium.config.yaml", b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n");
const APIS: (&str, &[u8]) = ("services/users/users.api.yaml", b"apis:\n  - id: get\n    path: /users/1\n");
const TESTS: (&str, &[u8]) = (
    "services/users/crud.test.yaml",
    b"tests:\n  - id: getsUser\n    call: users#get\n    expect:\n      custom:\n        - function: ../../hooks/h0.ts#check\n",
);

fn engine(files: &[(&str, &[u8])]) -> Engine {
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(files));
    let cache = Arc::new(MemoryCompileCache::new());
    EngineBuilder::new(PathBuf::from("/project")).sources(sources).compile_cache(cache).build().unwrap()
}

/// `hooks/h0.ts` to `hooks/h<last>.ts`, each importing the next; the last exports `check`.
fn chain(last: usize) -> Vec<(String, Vec<u8>)> {
    let hook = |at: usize| {
        let text = if at == last {
            "export function check() { return true; }\n".to_owned()
        } else {
            format!("export {{ check }} from \"./h{}.ts\";\n", at + 1)
        };
        (format!("hooks/h{at}.ts"), text.into_bytes())
    };
    (0..=last).map(hook).collect()
}

fn project(hooks: &[(String, Vec<u8>)]) -> Engine {
    let mut files: Vec<(&str, &[u8])> = vec![CONFIG, APIS, TESTS];
    files.extend(hooks.iter().map(|(path, text)| (path.as_str(), text.as_slice())));
    engine(&files)
}

#[test]
fn reads_each_hook_an_import_chain_reaches_for_eight_rounds() {
    let (program, _) = project(&chain(7)).program().unwrap();
    let paths: Vec<&str> = program.hooks.iter().map(|(path, _)| path).collect();
    assert_eq!(paths.len(), 8);
    assert_eq!((paths[0], paths[7]), ("hooks/h0.ts", "hooks/h7.ts"));
}

#[test]
fn a_chain_still_wanting_files_after_eight_rounds_is_a_clear_error() {
    let Err(EngineError::Sources { reason }) = project(&chain(8)).check(&CheckRequest::default()) else {
        panic!("nine files deep is one round too many");
    };
    assert!(reason.contains("after 8 rounds") && reason.ends_with("hooks/h8.ts; a hook's imports may be nested too deep"));
}

#[test]
fn a_wanted_file_that_cannot_be_read_is_v1012_once() {
    let hooks = [("hooks/h0.ts".to_owned(), vec![0xff, 0xfe])];
    let report = project(&hooks).check(&CheckRequest::default()).unwrap();
    let codes: Vec<Code> = report.diagnostics.iter().map(|found| found.code).collect();
    assert_eq!(codes, [Code::V1012]);
    assert_eq!(report.files.get(report.diagnostics.iter().next().unwrap().primary.span.file).unwrap().path.as_str(), "hooks/h0.ts");
}

#[test]
fn reads_the_data_files_the_project_names() {
    let report = engine(&[CONFIG, APIS, ("datasets/users.json", &[0xff])]).check(&CheckRequest::default()).unwrap();
    let codes: Vec<Code> = report.diagnostics.iter().map(|found| found.code).collect();
    assert_eq!(codes, [Code::V1012], "{:?}", report.diagnostics);
}
