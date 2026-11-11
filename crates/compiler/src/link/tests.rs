use vc_diagnostics::{Category, Code, Diagnostic};
use vc_model::{ContentHash, FileKind, SourceFile, SourcePath};

use super::project;
use crate::CompileInput;

const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

const APIS: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users/1\n  - id: make\n    method: POST\n    path: /users\n    save: {id: '${response.body.id}'}\n  - id: put\n    method: PUT\n    path: /users/${var.id}\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// Every diagnostic but the lifecycle lints, for a project with `config`, `apis` and the test
/// file `tests`.
fn diagnostics(config: &str, apis: &str, tests: &str) -> Vec<Diagnostic> {
    let files = vec![
        file("vibranium.config.yaml", config),
        file("services/users/users.api.yaml", apis),
        file("services/users/crud.test.yaml", tests),
    ];
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    compiled.diagnostics.iter().filter(|found| found.code.category() != Category::Lifecycle).cloned().collect()
}

/// Each diagnostic's code and line, in order.
fn found(tests: &str) -> Vec<(Code, u32)> {
    let mut found: Vec<_> =
        diagnostics(CONFIG, APIS, tests).iter().map(|found| (found.code, found.primary.span.start.line)).collect();
    found.sort_by_key(|(code, line)| (*line, *code));
    found
}

fn tests(body: &str) -> String {
    format!("tests:\n{body}")
}

#[test]
fn a_save_sets_its_name_for_the_steps_after_it() {
    let text = tests(concat!(
        "  - id: makes\n    call: users#make\n",
        "  - id: putsMade\n    steps:\n      - call: users#make\n      - call: users#put\n",
        "  - id: putsSaved\n    steps:\n      - call: users#get\n        save: {id: '${response.body.id}'}\n",
        "      - call: users#put\n",
        "  - id: putsReused\n    steps:\n      - test: makes\n      - call: users#put\n",
    ));
    assert_eq!(found(&text), []);
    assert_eq!(found(&tests("  - id: putsAlone\n    call: users#put\n")), [(Code::V2101, 11)]);
}

#[test]
fn a_step_id_can_be_read_after_that_step_runs() {
    let text = tests(
        "  - id: putsFetched\n    steps:\n      - id: fetch\n        call: users#get\n      - call: users#put\n        vars: {id: '${fetch.body.id}'}\n",
    );
    assert_eq!(found(&text), []);
}

#[test]
fn a_cache_check_reads_what_its_action_saved() {
    let check = "    cache:\n      for: 1h\n      check:\n        call: users#put\n";
    let shared =
        format!("beforeAll:\n  - id: made\n    call: users#make\n{check}        vars: {{id: '${{var.made.id}}'}}\n");
    let gets = tests("  - id: gets\n    call: users#get\n");
    assert_eq!(found(&format!("{shared}{gets}")), []);
    assert_eq!(found(&format!("beforeEach:\n  - call: users#make\n{check}{gets}")), []);
    assert_eq!(found(&format!("beforeEach:\n  - call: users#get\n{check}{gets}")), [(Code::V2101, 11)]);
}

#[test]
fn pipes_are_checked_against_the_type_every_definition_agrees_on() {
    let text = format!(
        "vars: {{on: true, map: {{a: 1}}, text: abc, copy: '${{var.on}}', n: 1}}\n{}",
        tests(concat!(
            "  - id: pipes\n    call: users#get\n    vars: {n: 2, id: 1}\n    headers:\n",
            "      x-on: '${var.on | upper}'\n",
            "      x-map: '${var.map | upper}'\n",
            "      x-text: '${var.text | shuffle}'\n",
            "      x-copy: '${var.copy | shuffle}'\n",
            "      x-n: '${var.n | upper}'\n",
            "      x-a: '${var.map.a | upper}'\n",
        ))
    );
    assert_eq!(found(&text), [(Code::V2304, 7), (Code::V2304, 8), (Code::V2304, 9), (Code::V2304, 11)]);
}

/// The label of the one diagnostic for `tests` under `config`.
fn label(config: &str, tests: &str) -> String {
    let [found] = &diagnostics(config, APIS, tests)[..] else { panic!("one diagnostic") };
    found.primary.text.clone()
}

#[test]
fn an_unset_var_names_the_profiles_that_miss_it() {
    let three = format!("{CONFIG}  - id: qa\n  - id: ci\n");
    let text = format!(
        "profiles:\n  - id: dev\n    vars: {{x: 1}}\n{}",
        tests("  - id: a\n    call: users#get\n    headers: {x-x: '${var.x}'}\n")
    );
    assert_eq!(label(&three, &text), "no level sets x for profile qa, ci");
    let unset = tests("  - id: a\n    call: users#get\n    headers: {x-y: '${var.y}'}\n");
    assert_eq!(label(CONFIG, &unset), "no level sets y for profile dev");
    let two = format!("{CONFIG}  - id: qa\n");
    assert_eq!(label(&two, &unset), "no level sets y for any profile");
}

#[test]
fn vars_in_expected_matchers_are_checked() {
    let text = tests(
        "  - id: a\n    call: users#get\n    expect:\n      headers:\n        x-a: '${var.one}'\n        x-b: {contains: '${var.two}'}\n",
    );
    assert_eq!(found(&text), [(Code::V2101, 6), (Code::V2101, 7)]);
}

#[test]
fn an_unset_var_read_through_default_is_not_reported_but_through_another_pipe_is() {
    let header = |pipe: &str| tests(&format!("  - id: a\n    call: users#get\n    headers: {{x-note: \"${{var.note | {pipe}}}\"}}\n"));
    assert_eq!(found(&header("default('none')")), []);
    assert_eq!(found(&header("lower")), [(Code::V2101, 4)]);
}

#[test]
fn a_flow_expiry_reads_what_its_steps_saved() {
    let flow = |expiry: &str| {
        format!(
            "{CONFIG}auth:\n  - id: login\n    type: flow\n    steps:\n      - id: token\n        method: POST\n        url: http://x/token\n        save: {{ttl: '${{response.body.ttl}}'}}\n    apply: {{headers: {{authorization: 'Bearer ${{var.ttl}}'}}}}\n    expiresIn: '${{var.{expiry}}}s'\n"
        )
    };
    let unset = |config: &str| {
        let calls = tests("  - id: a\n    call: users#get\n");
        diagnostics(config, APIS, &calls).iter().filter(|found| found.code == Code::V2101).count()
    };
    assert_eq!(unset(&flow("ttl")), 0);
    assert_eq!(unset(&flow("nope")), 1);
}

#[test]
fn an_auth_without_a_key_its_type_needs_is_reported_at_its_type() {
    let auth = |keys: &str| format!("{CONFIG}auth:\n  - id: partner\n    type: bearer\n{keys}");
    let missing = |config: &str| {
        let found = diagnostics(config, APIS, &tests("  - id: a\n    call: users#get\n"));
        found.iter().map(|found| (found.code, found.primary.span.start.line, found.message.clone())).collect::<Vec<_>>()
    };
    assert_eq!(missing(&auth("")), [(Code::V1005, 9, "a `bearer` auth needs `token`".to_owned())]);
    assert_eq!(missing(&auth("    token: ${var.baseUrl}\n")), []);
}

/// The new text of the fix for each diagnostic.
fn fixes(tests: &str) -> Vec<String> {
    let found = diagnostics(CONFIG, APIS, tests);
    found.iter().flat_map(|found| &found.fixes).flat_map(|fix| &fix.edits).map(|edit| edit.new_text.clone()).collect()
}

#[test]
fn a_close_id_is_offered_as_it_was_written() {
    let text = tests(
        "  - id: readsUser\n    call: users#get\n  - id: followsTheWholeChain\n    steps:\n      - test: readsUsr\n      - test: users.crud#readsUsr\n",
    );
    assert_eq!(fixes(&text), ["readsUser", "users.crud#readsUser"]);
}

#[test]
fn references_into_a_file_that_could_not_be_read_are_not_reported() {
    let broken = format!("{APIS}    colour: red\n");
    let text = tests("  - id: a\n    call: users#nope\n");
    let codes: Vec<Code> = diagnostics(CONFIG, &broken, &text).iter().map(|found| found.code).collect();
    assert_eq!(codes, [Code::V1004]);
    let codes: Vec<Code> = diagnostics(CONFIG, APIS, &text).iter().map(|found| found.code).collect();
    assert_eq!(codes, [Code::V2003]);
}

#[test]
fn a_file_id_drops_the_root_and_suffix_and_collapses_an_api_named_like_its_folder() {
    assert_eq!(project::file_id("services/users/users.api.yaml", FileKind::Api), "users");
    assert_eq!(project::file_id("services/users/admin.api.yaml", FileKind::Api), "users.admin");
    assert_eq!(project::file_id("services/users/users.test.yaml", FileKind::Test), "users.users");
    assert_eq!(
        project::file_id("services/payments/refunds/partial.test.yaml", FileKind::Test),
        "payments.refunds.partial"
    );
    assert_eq!(project::file_id("load/checkout.load.yaml", FileKind::Load), "checkout");
}

#[test]
fn report_formats_name_a_built_in_format_or_a_reporter_plugin() {
    let plugins = "plugins:\n  - id: acmeReport\n    source: path:report\n";
    let config = format!("{CONFIG}{plugins}reports:\n  formats: [junit, acmeReport, acmeReprt]\n");
    let test = tests("  - id: getsUser\n    call: users#get\n");
    let found = diagnostics(&config, APIS, &test);
    let [wrong] = found.as_slice() else { panic!("one diagnostic, not {found:?}") };
    assert_eq!(wrong.code, Code::V2302);
    assert_eq!(wrong.message, "expected one of ctrf, junit, html or a reporter plugin's ID, found `acmeReprt`");
    assert_eq!(wrong.help, ["did you mean `acmeReport`?"]);
    let without = format!("{CONFIG}reports:\n  formats: [acmeReport]\n");
    assert_eq!(diagnostics(&without, APIS, &test).iter().map(|found| found.code).collect::<Vec<_>>(), [Code::V2302]);
}
