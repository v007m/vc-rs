use std::collections::HashSet;

use vc_diagnostics::{Category, Code};
use vc_model::{ContentHash, SourceFile, SourcePath};

use super::LintRegistry;
use crate::CompileInput;

const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

const APIS: &str = "name: Users\nbaseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users/1\n  - id: head\n    method: HEAD\n    path: /users/1\n  - id: make\n    method: POST\n    path: /users\n    save: {id: '${response.body.id}'}\n  - id: put\n    method: PUT\n    path: /users/${var.id}\n  - id: drop\n    method: DELETE\n    path: /users/${var.id}\n  - id: old\n    path: /old\n    deprecated: true\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// The lifecycle diagnostics, as code and line, for a project with `config` and the test file
/// `tests`, which has no other error or warning but conventions (its IDs are short).
fn found_with(config: &str, tests: &str) -> Vec<(Code, u32)> {
    let compiled = compiled(config, tests);
    let lifecycle = compiled.diagnostics.iter().filter(|found| found.code.category() == Category::Lifecycle);
    let mut found: Vec<(Code, u32)> = lifecycle.map(|found| (found.code, found.primary.span.start.line)).collect();
    found.sort_by_key(|(code, line)| (*line, *code));
    let others = compiled
        .diagnostics
        .iter()
        .filter(|found| !matches!(found.code.category(), Category::Lifecycle | Category::Convention));
    assert_eq!(others.count(), 0, "{:?}", compiled.diagnostics);
    found
}

fn found(tests: &str) -> Vec<(Code, u32)> {
    found_with(CONFIG, tests)
}

fn tests(body: &str) -> String {
    format!("name: Crud\ntests:\n{body}")
}

#[test]
fn the_registry_holds_one_lint_per_lifecycle_or_check_time_value_code() {
    let codes: Vec<Code> = LintRegistry::builtin().lints.iter().map(|lint| lint.code()).collect();
    assert_eq!(codes.len(), 14);
    assert_eq!(codes.iter().collect::<HashSet<_>>().len(), codes.len());
    assert!(codes.iter().all(|code| matches!(code.category(), Category::Lifecycle | Category::Reference)));
}

#[test]
fn a_clean_project_has_no_lifecycle_diagnostics() {
    let text = tests(
        "  - id: readsUser\n    call: users#get\n  - id: makesUser\n    body: {name: a}\n    headers: {x-a: '1'}\n    expect: {status: 201}\n    steps:\n      - call: users#make\n      - call: users#make\n        headers: {x-a: '2'}\n",
    );
    assert_eq!(found(&text), []);
}

#[test]
fn a_second_default_profile_is_reported_at_its_flag() {
    let config = format!("{CONFIG}  - id: qa\n    default: false\n  - id: ci\n    default: true\n");
    assert_eq!(found_with(&config, &tests("  - id: readsUser\n    call: users#get\n")), [(Code::V3102, 10)]);
}

#[test]
fn a_blank_skip_reason_is_reported_and_a_written_one_is_not() {
    let text = format!(
        "name: Crud\nskip: {{reason: ' '}}\ntests:\n{}",
        "  - id: a\n    call: users#get\n    skip: ''\n  - id: b\n    call: users#get\n    skip: flaky upstream\n  - id: c\n    call: users#get\n    skip: {reason: see 212}\n"
    );
    assert_eq!(found(&text), [(Code::V3103, 2), (Code::V3103, 6)]);
}

/// A file skipped until `file_until`, holding a test skipped until `test_until`, compiled on 2026-11-03.
fn expired(file_until: &str, test_until: &str) -> crate::Compiled {
    let text = format!(
        "name: Crud\nskip: {{reason: down, until: {file_until}}}\ntests:\n  - id: getsUser\n    call: users#get\n    skip:\n      reason: flaky\n      until: {test_until}\n  - id: getsAgain\n    call: users#get\n"
    );
    let mut input = input(CONFIG, &text);
    input.today = vc_model::Date::parse("2026-11-03");
    crate::compile(input)
}

#[test]
fn a_skip_whose_until_has_passed_is_reported_with_a_fix_that_removes_it() {
    let compiled = expired("2026-11-02", "2026-01-31");
    let found: Vec<_> = compiled.diagnostics.iter().map(|d| (d.code, d.primary.span.start.line)).collect();
    assert_eq!(found, [(Code::V3104, 2), (Code::V3104, 8)]);
    let text = &compiled.files.get(vc_model::SourceFileId::new(0)).unwrap().text;
    let removed: Vec<&str> = compiled.diagnostics.iter().map(|d| &text[d.fixes[0].edits[0].range.clone()]).collect();
    assert_eq!(
        removed,
        ["skip: {reason: down, until: 2026-11-02}\n", "    skip:\n      reason: flaky\n      until: 2026-01-31\n"]
    );
    assert!(compiled.program.is_some(), "an expired skip is a warning");
}

#[test]
fn removing_a_skip_removes_a_comment_after_it_too() {
    let compiled = expired("2026-11-03", "2026-01-31  # see 212");
    let text = &compiled.files.get(vc_model::SourceFileId::new(0)).unwrap().text;
    let removed: Vec<&str> = compiled.diagnostics.iter().map(|d| &text[d.fixes[0].edits[0].range.clone()]).collect();
    assert_eq!(removed, ["    skip:\n      reason: flaky\n      until: 2026-01-31  # see 212\n"]);
}

#[test]
fn a_skip_until_today_or_later_or_with_no_date_given_is_not_reported() {
    assert!(expired("2026-11-03", "2027-01-01").diagnostics.is_empty());
    assert_eq!(found(&tests("  - id: a\n    call: users#get\n    skip: {reason: x, until: 2020-01-01}\n")), []);
}

#[test]
fn a_skip_on_a_line_it_shares_has_no_fix() {
    let text = "name: Crud\ntests:\n  - {id: getsUser, call: users#get, skip: {reason: x, until: 2020-01-01}}\n";
    let mut input = input(CONFIG, text);
    input.today = vc_model::Date::parse("2026-11-03");
    let compiled = crate::compile(input);
    let found: Vec<_> = compiled.diagnostics.iter().map(|d| (d.code, d.fixes.len())).collect();
    assert_eq!(found, [(Code::V3104, 0)]);
}

#[test]
fn an_allow_failure_whose_until_has_passed_is_v4008_with_a_fix_that_removes_it() {
    let text = concat!(
        "name: Crud\nallowFailure: {reason: down, until: 2026-11-02}\ntests:\n",
        "  - id: getsUser\n    call: users#get\n    allowFailure:\n      reason: flaky\n      until: 2026-01-31\n",
        "  - id: getsToday\n    call: users#get\n    allowFailure: {reason: x, until: 2026-11-03}\n",
        "  - id: getsAny\n    call: users#get\n    allowFailure: true\n",
    );
    let mut input = input(CONFIG, text);
    input.today = vc_model::Date::parse("2026-11-03");
    let compiled = crate::compile(input);
    let found: Vec<_> = compiled.diagnostics.iter().map(|d| (d.code, d.primary.span.start.line)).collect();
    assert_eq!(found, [(Code::V4008, 2), (Code::V4008, 8)], "{:?}", compiled.diagnostics);
    let text = &compiled.files.get(vc_model::SourceFileId::new(0)).unwrap().text;
    let removed: Vec<&str> = compiled.diagnostics.iter().map(|d| &text[d.fixes[0].edits[0].range.clone()]).collect();
    assert_eq!(
        removed,
        [
            "allowFailure: {reason: down, until: 2026-11-02}\n",
            "    allowFailure:\n      reason: flaky\n      until: 2026-01-31\n"
        ]
    );
    assert!(compiled.program.is_some(), "an expired allowFailure is a warning");
}

#[test]
fn an_allow_failure_needs_a_reason_in_its_object_form_and_a_bool_otherwise() {
    let found = |setting: &str| {
        let compiled =
            compiled(CONFIG, &tests(&format!("  - id: a\n    call: users#get\n    allowFailure: {setting}\n")));
        let shown = compiled.diagnostics.iter().filter(|d| d.code.category() != Category::Convention);
        shown.map(|d| d.code).collect::<Vec<_>>()
    };
    assert_eq!(found("false"), []);
    assert_eq!(found("{reason: flaky}"), []);
    assert_ne!(found("{until: 2027-01-01}"), [], "a dated allowFailure says why");
    assert_ne!(found("sometimes"), []);
    assert_ne!(found("{reason: x, until: soon}"), [], "`until` is a date");
}

#[test]
fn a_write_that_sends_a_shared_value_is_reported() {
    let text = format!(
        "name: Crud\nbeforeAll:\n  - id: seed\n    call: users#make\ntests:\n{}",
        "  - id: a\n    call: users#put\n    vars: {id: '${var.seed.id}'}\n  - id: b\n    call: users#get\n    headers: {x-id: '${var.seed.id}'}\n  - id: c\n    before:\n      - call: users#drop\n        vars: {id: '${var.seed.id}'}\n    call: users#get\n"
    );
    assert_eq!(found(&text), [(Code::V3201, 8), (Code::V3201, 15)]);
}

#[test]
fn a_write_that_sends_its_own_value_is_not_reported() {
    let text = format!(
        "name: Crud\nbeforeAll:\n  - id: seed\n    call: users#make\ntests:\n{}",
        "  - id: a\n    steps:\n      - id: made\n        call: users#make\n      - call: users#put\n        vars: {id: '${var.id}'}\n"
    );
    assert_eq!(found(&text), []);
}

#[test]
fn cache_is_reported_on_per_test_actions_only() {
    let text = format!(
        "name: Crud\nbeforeAll:\n  - id: seed\n    call: users#make\n    cache: {{for: 1h}}\nbeforeEach:\n  - call: users#make\n    cache: {{for: 1h}}\ntests:\n{}",
        "  - id: a\n    before:\n      - call: users#make\n        cache: {for: 1h}\n    call: users#get\n"
    );
    assert_eq!(found(&text), [(Code::V3202, 8), (Code::V3202, 13)]);
}

#[test]
fn a_call_to_a_deprecated_api_is_reported_where_it_is_written() {
    let text = tests(
        "  - id: a\n    call: users#old\n  - id: b\n    steps:\n      - call: users#get\n      - call: users#old\n",
    );
    assert_eq!(found(&text), [(Code::V3203, 4), (Code::V3203, 8)]);
}

#[test]
fn a_body_default_is_reported_at_each_read_step_without_its_own() {
    let text = tests(
        "  - id: a\n    body: {n: 1}\n    steps:\n      - call: users#make\n      - call: users#get\n      - call: users#head\n      - call: users#get\n        body: {n: 2}\n",
    );
    assert_eq!(found(&text), [(Code::V3205, 7), (Code::V3205, 8)]);
}

#[test]
fn defaults_every_call_step_replaces_are_unused() {
    let text = tests(concat!(
        "  - id: a\n    auth: none\n    body: {n: 1}\n    headers: {x-a: '1', x-b: '2'}\n    query: {q: '1'}\n",
        "    expect: {status: 201, size: {lt: 1kb}}\n    steps:\n",
        "      - call: users#make\n        auth: none\n        body: {n: 2}\n        headers: {x-a: '3'}\n        expect: {status: 200}\n",
        "      - test: users.crud#b\n      - function: hooks/a.ts#check\n  - id: b\n    call: users#make\n",
    ));
    assert_eq!(found(&text), [(Code::V3206, 4), (Code::V3206, 5), (Code::V3206, 6), (Code::V3206, 8)]);
}

#[test]
fn defaults_of_a_test_whose_steps_take_none_are_unused() {
    let text = tests(concat!(
        "  - id: a\n    headers: {x-a: '1'}\n    expect: {status: 200}\n    steps:\n      - function: hooks/a.ts#check\n",
        "      - test: users.crud#b\n  - id: b\n    call: users#make\n",
    ));
    assert_eq!(found(&text), [(Code::V3206, 4), (Code::V3206, 5)], "D270: a `test:` step takes no defaults");
}

#[test]
fn keys_a_test_action_ignores_are_reported_with_a_fix_that_removes_them() {
    let text = concat!(
        "name: Crud\nbeforeAll:\n  - id: seed\n    test: users.crud#b\n    query: {q: '1'}\ntests:\n",
        "  - id: a\n    steps:\n      - test: users.crud#b\n        headers: {x-a: '1'}\n        expect:\n",
        "          status: 202\n        timeout: 2s\n      - {test: users.crud#b, auth: none}\n",
        "  - id: b\n    call: users#make\n",
    );
    assert_eq!(found(text), [(Code::V3207, 5), (Code::V3207, 10), (Code::V3207, 11), (Code::V3207, 14)]);
    let compiled = compiled(CONFIG, text);
    let fixes = compiled.diagnostics.iter().filter(|found| found.code == Code::V3207).map(|found| found.fixes.len());
    assert_eq!(fixes.collect::<Vec<_>>(), [1, 1, 1, 0], "no fix for a key that shares its line");
}

fn input(config: &str, tests: &str) -> CompileInput {
    let files = vec![
        file("vibranium.config.yaml", config),
        file("services/users/users.api.yaml", APIS),
        file("services/users/crud.test.yaml", tests),
    ];
    CompileInput { files, ..CompileInput::default() }
}

fn compiled(config: &str, tests: &str) -> crate::Compiled {
    crate::compile(input(config, tests))
}

#[test]
fn lints_wait_until_the_project_has_no_errors() {
    let config = format!("{CONFIG}  - id: ci\n    default: true\n");
    let compiled = compiled(&config, &tests("  - id: a\n    call: users#nope\n"));
    let codes: Vec<Code> = compiled.diagnostics.iter().map(|found| found.code).collect();
    assert_eq!(codes, [Code::V2003]);
    assert!(compiled.program.is_none());
}

#[test]
fn a_lint_error_drops_the_program_and_a_warning_keeps_it() {
    let config = format!("{CONFIG}  - id: ci\n    default: true\n");
    let error = compiled(&config, &tests("  - id: a\n    call: users#get\n"));
    assert!(error.diagnostics.has_errors() && error.program.is_none());
    let warning = compiled(CONFIG, &tests("  - id: a\n    call: users#old\n"));
    assert!(!warning.diagnostics.is_empty() && warning.program.is_some());
}
