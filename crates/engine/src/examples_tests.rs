use std::sync::Arc;

use vc_compiler::CompileInput;
use vc_executor::RunInputs;
use vc_model::{ContentHash, Program, SourceFile, SourcePath};
use vc_planner::{PlanOptions, Selection};
use vc_testkit::{MemoryEnvironment, ScriptedRandom};

use super::{Draw, planned};
use crate::EngineError;

const CONFIG: &str = "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";
const APIS: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// A project whose one test reads its rows from a `$file` the compile was not given.
fn unloaded_file() -> Program {
    let tests = "tests:\n  - id: listsPages\n    call: users#get\n    examples: {$file: pages.csv}\n";
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", APIS),
        file("services/users/rows.test.yaml", tests),
    ];
    let compiled = vc_compiler::compile(CompileInput { files, ..CompileInput::default() });
    let Some(program) = compiled.program else { panic!("{:?}", compiled.diagnostics) };
    Arc::unwrap_or_clone(program)
}

#[test]
fn rows_from_a_file_the_compile_did_not_give_are_not_loaded_rather_than_rendered() {
    let program = unloaded_file();
    let inputs = RunInputs::new(Arc::new(ScriptedRandom::seeded(7)), Arc::new(MemoryEnvironment::default()));
    let mut draw = Draw::new(Some(7));
    let refused = planned(&program, &Selection::default(), (inputs, &mut draw, PlanOptions::default()));
    let Err(EngineError::Selection { reason, .. }) = refused else { panic!("a selection error expected: {refused:?}") };
    assert_eq!(reason, "the example rows of `users.rows#listsPages` were not loaded");
    assert!(draw.rows.is_empty(), "nothing was rendered");
}
