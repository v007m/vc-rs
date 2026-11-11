use std::sync::Arc;

use vc_compiler::CompileInput;
use vc_executor::RunInputs;
use vc_model::{ContentHash, Names, Program, SourceFile, SourcePath};
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

/// The labels a run of `tests` gives its instances, in plan order, `--repeat` times when given.
fn labels(tests: &str, repeat: Option<u32>) -> Vec<String> {
    let config = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars:\n  baseUrl: 'http://x'\n  pages: [{name: first page, page: 1}, {page: 2}]\n";
    let files = vec![
        file("vibranium.config.yaml", config),
        file("services/users/users.api.yaml", APIS),
        file("services/users/rows.test.yaml", tests),
    ];
    let compiled = vc_compiler::compile(CompileInput { files, ..CompileInput::default() });
    let Some(program) = compiled.program else { panic!("{:?}", compiled.diagnostics) };
    let inputs = RunInputs::new(Arc::new(ScriptedRandom::seeded(7)), Arc::new(MemoryEnvironment::default()));
    let options = PlanOptions { repeat, ..PlanOptions::default() };
    let plan = planned(&program, &Selection::default(), (inputs, &mut Draw::new(Some(7)), options)).unwrap();
    let names = Names::new(program, &plan);
    names.planned().iter().map(|planned| names.label(planned.test, Some(planned.instance))).collect()
}

#[test]
fn an_example_row_with_a_name_is_labeled_by_it() {
    let tests = "tests:\n  - id: rejectsBadEmail\n    call: users#get\n    examples:\n      - {name: missing @, email: a.b}\n      - {email: '@b'}\n      - {name: '${var.baseUrl}', email: c}\n  - id: listsPages\n    call: users#get\n    examples: ${var.pages}\n";
    let expected = [
        "users.rows#rejectsBadEmail › missing @",
        "users.rows#rejectsBadEmail [row 2]",
        "users.rows#rejectsBadEmail [row 3]",
        "users.rows#listsPages › first page",
        "users.rows#listsPages [row 2]",
    ];
    assert_eq!(labels(tests, None), expected, "a templated name in a written row renders only as it runs");
}

#[test]
fn rows_that_share_a_name_keep_their_numbers_so_no_two_labels_match() {
    let tests = "tests:\n  - id: createsUsers\n    call: users#get\n    examples:\n      - {name: Bob, email: a}\n      - {name: Ann, email: b}\n      - {name: Bob, email: c}\n";
    let expected = [
        "users.rows#createsUsers › Bob [row 1]",
        "users.rows#createsUsers › Ann",
        "users.rows#createsUsers › Bob [row 3]",
    ];
    assert_eq!(labels(tests, None), expected, "only the rows named alike show their numbers");
    let repeated = labels(tests, Some(2));
    assert_eq!(repeated[0], "users.rows#createsUsers › Bob [row 1, repeat 1]", "{repeated:?}");
}

#[test]
fn every_instance_of_a_repeated_test_shows_its_repeat_however_it_repeats() {
    let tests =
        "tests:\n  - id: getsUsers\n    call: users#get\n  - id: getsAgain\n    call: users#get\n    repeat: 2\n";
    let flag = ["users.rows#getsUsers [repeat 1]", "users.rows#getsUsers [repeat 2]"];
    assert_eq!(labels(tests, Some(2))[..2], flag, "`--repeat 2` labels the first run too");
    let key = ["users.rows#getsUsers", "users.rows#getsAgain [repeat 1]", "users.rows#getsAgain [repeat 2]"];
    assert_eq!(labels(tests, None), key, "a test run once shows no repeat");
}
