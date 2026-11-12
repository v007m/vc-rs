use std::collections::BTreeMap;

use vc_model::{ContentHash, SourceFile, SourcePath};

use super::{Wanted, WantedError, WantedRead, compile_wanted};
use crate::CompileInput;

const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n";
const APIS: &str = "apis:\n  - id: get\n    path: /users/1\n";
const TESTS: &str = "tests:\n  - id: getsUser\n    call: users#get\n    expect:\n      custom:\n        - function: ../../hooks/h0.ts#check\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

fn input(config: &str) -> CompileInput {
    let files = vec![
        file("vibranium.config.yaml", config),
        file("services/users/users.api.yaml", APIS),
        file("services/users/crud.test.yaml", TESTS),
    ];
    CompileInput { files, ..CompileInput::default() }
}

/// `hooks/h0.ts` to `hooks/h<last>.ts`, each importing the next; the last exports `check`.
fn chain(last: usize) -> BTreeMap<String, String> {
    let hook = |at: usize| {
        let text = if at == last {
            "export function check() { return true; }\n".to_owned()
        } else {
            format!("export {{ check }} from \"./h{}.ts\";\n", at + 1)
        };
        (format!("hooks/h{at}.ts"), text)
    };
    (0..=last).map(hook).collect()
}

/// A reader over `files` that records every ask; a file it lacks is a problem naming its path.
fn reader<'a>(
    files: &'a BTreeMap<String, String>,
    asks: &'a mut Vec<Vec<Wanted>>,
) -> impl FnMut(&[Wanted]) -> Result<WantedRead<String>, String> + 'a {
    move |wanted| {
        asks.push(wanted.to_vec());
        let (found, missing): (Vec<&Wanted>, Vec<&Wanted>) =
            wanted.iter().partition(|(path, _)| files.contains_key(path.as_str()));
        let read = found.into_iter().map(|(path, _)| file(path.as_str(), &files[path.as_str()])).collect();
        Ok((read, missing.into_iter().map(|(path, _)| path.as_str().to_owned()).collect()))
    }
}

/// A chain is followed through its imports, which only oxc (the `hooks` feature) reads.
#[test]
#[cfg(feature = "hooks")]
fn reads_each_hook_an_import_chain_reaches_one_level_a_round() {
    let (hooks, mut asks) = (chain(7), Vec::new());
    let (compiled, problems) = compile_wanted(input(CONFIG), reader(&hooks, &mut asks)).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    let program = compiled.program.unwrap();
    assert_eq!(program.hooks.iter().count(), 8);
    let asked: Vec<&str> = asks.iter().flatten().map(|(path, _)| path.as_str()).collect();
    assert_eq!(asked, hooks.keys().map(String::as_str).collect::<Vec<_>>());
}

#[test]
#[cfg(feature = "hooks")]
fn a_chain_still_wanting_files_after_eight_rounds_is_too_deep() {
    let (hooks, mut asks) = (chain(8), Vec::new());
    let Err(error) = compile_wanted(input(CONFIG), reader(&hooks, &mut asks)) else { panic!("one round too many") };
    assert_eq!(error, WantedError::TooDeep(vec![SourcePath::new("hooks/h8.ts").unwrap()]));
    assert_eq!(asks.len(), 8);
    let message = "the project still names files after 8 rounds of reading them: hooks/h8.ts; a hook's imports may be nested too deep";
    assert_eq!(error.to_string(), message);
    assert_eq!(error.into_error(|reason| format!("refused: {reason}")), format!("refused: {message}"));
}

#[test]
fn a_file_that_cannot_be_read_is_asked_for_once_and_comes_back_as_a_problem() {
    let (none, mut asks) = (BTreeMap::new(), Vec::new());
    let (compiled, problems) = compile_wanted(input(CONFIG), reader(&none, &mut asks)).unwrap();
    assert_eq!(problems, ["hooks/h0.ts"]);
    assert_eq!(asks.len(), 1);
    assert!(compiled.hooks_wanted.iter().any(|path| path.as_str() == "hooks/h0.ts"));
}

#[test]
fn an_outside_data_file_is_asked_for_with_its_allowed_root() {
    let config = format!(
        "{CONFIG}datasets:\n  allowRoots: [../shared]\n  files:\n    - id: shared\n      file: ../shared/ok.csv\n"
    );
    let (hooks, mut asks) = (chain(0), Vec::new());
    let (_, problems) = compile_wanted(input(&config), reader(&hooks, &mut asks)).unwrap();
    assert_eq!(problems, ["../shared/ok.csv"]);
    let outside = (SourcePath::data("../shared/ok.csv").unwrap(), Some(SourcePath::data("../shared").unwrap()));
    assert_eq!(asks[0], [outside, (SourcePath::new("hooks/h0.ts").unwrap(), None)]);
}

#[test]
fn a_reader_that_fails_stops_the_loop_with_its_error() {
    let failing = |_: &[Wanted]| -> Result<WantedRead<String>, String> { Err("the disk is gone".to_owned()) };
    let Err(error) = compile_wanted(input(CONFIG), failing) else { panic!("the reader failed") };
    assert_eq!(error, WantedError::Read("the disk is gone".to_owned()));
    assert_eq!(error.to_string(), "the disk is gone");
    assert_eq!(error.into_error(|_| "too deep".to_owned()), "the disk is gone");
}

#[test]
fn a_project_that_wants_nothing_is_compiled_once_without_reading() {
    let tests = "tests:\n  - id: getsUser\n    call: users#get\n";
    let mut files = input(CONFIG).files;
    files[2] = file("services/users/crud.test.yaml", tests);
    let read = |_: &[Wanted]| -> Result<WantedRead<String>, String> { Err("asked".to_owned()) };
    let (compiled, problems) = compile_wanted(CompileInput { files, ..CompileInput::default() }, read).unwrap();
    assert!(compiled.program.is_some() && problems.is_empty());
}
