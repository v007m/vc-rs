use vc_diagnostics::{Category, Code, Diagnostic};
use vc_model::{ContentHash, SourceFile, SourcePath};

use crate::CompileInput;

/// A config whose `token` reads a secret, so it has no value before the run.
const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nvars:\n  baseUrl: http://x\n  token: ${secret.API_TOKEN}\n  cities: [{name: Oslo}, {name: Lima}]\nprofiles:\n  - id: dev\n    default: true\n";

const APIS: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users\n  - id: make\n    method: POST\n    path: /users\n    save: {id: '${response.body.id}'}\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// The V2105s of a project with the test file `tests`, with each one's line.
fn unavailable(tests: &str) -> Vec<(u32, Diagnostic)> {
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", APIS),
        file("services/users/crud.test.yaml", tests),
    ];
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    let found = compiled.diagnostics.iter().filter(|found| found.code.category() != Category::Lifecycle);
    found
        .filter(|found| found.code == Code::V2105)
        .map(|found| (found.primary.span.start.line, found.clone()))
        .collect()
}

/// The help of the one V2105 in `tests`, which is on `line`.
fn help_at(tests: &str, line: u32) -> String {
    let found = unavailable(tests);
    let [(at, diagnostic)] = found.as_slice() else { panic!("one V2105 expected: {found:?}") };
    assert_eq!(*at, line, "{diagnostic:?}");
    diagnostic.help.join("\n")
}

#[test]
fn an_examples_template_may_not_read_a_var_that_reads_a_secret() {
    let tests = "tests:\n  - id: listsWithToken\n    call: users#get\n    examples: ${var.token}\n";
    let help = help_at(tests, 4);
    assert!(help.starts_with("`var.token` needs `secret`;"), "{help}");
}

#[test]
fn an_examples_template_follows_the_vars_a_var_reads() {
    let tests = "tests:\n  - id: listsPicked\n    call: users#get\n    vars: {picked: '${var.token}'}\n    examples: ${var.picked}\n";
    let help = help_at(tests, 5);
    assert!(help.starts_with("`var.token` needs `secret`;"), "the var that reads the secret: {help}");
}

#[test]
fn an_examples_template_may_not_read_what_before_all_saves() {
    let tests = "beforeAll:\n  - id: seeded\n    call: users#make\ntests:\n  - id: listsSeeded\n    call: users#get\n    examples: ${var.seeded.id}\n";
    let help = help_at(tests, 7);
    assert!(help.starts_with("`var.seeded` needs a save"), "{help}");
}

#[test]
fn vars_set_before_the_run_and_inline_rows_are_not_flagged() {
    let tests = concat!(
        "tests:\n  - id: listsCities\n    call: users#get\n    examples: ${var.cities}\n",
        "  - id: listsNear\n    call: users#get\n    vars: {near: '${var.cities}'}\n    examples: ${var.near}\n",
        "  - id: listsInline\n    call: users#get\n    examples: [{auth: '${var.token}'}]\n",
    );
    assert_eq!(unavailable(tests), []);
}
