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
    unavailable_with(CONFIG, tests)
}

/// The V2105s of a project with the config `config` and the test file `tests`.
fn unavailable_with(config: &str, tests: &str) -> Vec<(u32, Diagnostic)> {
    let files = vec![
        file("vibranium.config.yaml", config),
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

/// A config whose base `vars` are `vars`.
fn config_vars(vars: &str) -> String {
    format!("syntaxVersion: 2\nproject: shop\nvars:\n  baseUrl: http://x\n{vars}")
}

/// One test with its own `vars` whose `examples` reads `var.a`, on line 5.
fn reads_a(vars: &str) -> String {
    format!("tests:\n  - id: listsA\n    call: users#get\n    vars: {vars}\n    examples: ${{var.a}}\n")
}

/// The help of the one V2105, which is at `examples` on line 5.
fn help_on_5(found: &[(u32, Diagnostic)]) -> String {
    let [(line, diagnostic)] = found else { panic!("one V2105 expected: {found:?}") };
    assert_eq!(*line, 5, "{diagnostic:?}");
    diagnostic.help.join("\n")
}

#[test]
fn a_var_read_by_a_config_var_is_looked_up_from_the_test_out() {
    let config = config_vars("  a: ${var.b}\n  b: lit\n");
    let help = help_on_5(&unavailable_with(&config, &reads_a("{b: '${secret.S}'}")));
    assert!(help.starts_with("`var.b` needs `secret`;"), "the test's `b`, as the engine reads it: {help}");
    let shadowed = config_vars("  a: ${var.b}\n  b: ${secret.S}\n");
    assert_eq!(unavailable_with(&shadowed, &reads_a("{b: lit}")), []);
}

#[test]
fn a_var_that_reads_its_own_name_reads_the_next_level_out() {
    let config = config_vars("  a: ${secret.S}\n");
    let help = help_on_5(&unavailable_with(&config, &reads_a("{a: 'x${var.a}'}")));
    assert!(help.starts_with("`var.a` needs `secret`;"), "the config's `a`: {help}");
    let literal = config_vars("  a: lit\n");
    assert_eq!(unavailable_with(&literal, &reads_a("{a: 'x${var.a}'}")), []);
}

#[test]
fn a_profile_definition_is_left_to_the_engine() {
    let config = config_vars("  rows: [{n: 1}]\nprofiles:\n  - id: staging\n    vars:\n      rows: ${secret.S}\n");
    let tests = "tests:\n  - id: listsRows\n    call: users#get\n    examples: ${var.rows}\n";
    assert_eq!(unavailable_with(&config, tests), []);
}

#[test]
fn a_file_of_rows_has_a_path_written_as_it_is() {
    let tests = "tests:\n  - id: listsRegion\n    call: users#get\n    examples: {$file: 'rows/${env.REGION}.yaml'}\n";
    let found = unavailable(tests);
    let [(line, diagnostic)] = found.as_slice() else { panic!("one V2105 expected: {found:?}") };
    assert_eq!(*line, 4);
    assert_eq!(diagnostic.message, "the `$file` path of `examples` holds a template");
    assert!(diagnostic.help.join("\n").contains("`examples: ${var.rows}`"), "{diagnostic:?}");
}

#[test]
fn a_default_still_reads_a_defined_var_but_covers_one_only_a_save_sets() {
    let tests = "tests:\n  - id: listsWithToken\n    call: users#get\n    examples: ${var.token | default('x')}\n";
    let help = help_at(tests, 4);
    assert!(help.starts_with("`var.token` needs `secret`;"), "the rows render `token`'s definition: {help}");
    let saved = "beforeAll:\n  - id: seeded\n    call: users#make\ntests:\n  - id: listsSeeded\n    call: users#get\n    examples: ${var.seeded | default('x')}\n";
    assert_eq!(unavailable(saved), [], "before the run the save has not run, so the default is given");
}
