use vc_diagnostics::{Category, Code};
use vc_model::{ContentHash, SourceFile, SourcePath};

use crate::CompileInput;

const CONFIG: (&str, &str) = (
    "vibranium.config.yaml",
    "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n",
);

const USERS: (&str, &str) =
    ("services/users/users.api.yaml", "baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users/1\n");

/// One convention warning: its code, its message, and each edit of its fix as the text it
/// replaces and the text it writes.
type Found = (Code, String, Vec<(String, String)>);

/// The V4 warnings for a project of `files`, sorted, after asserting it has no errors.
fn found(files: &[(&str, &str)]) -> Vec<Found> {
    let files = files.iter().map(|(path, text)| file(path, text)).collect();
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    assert!(compiled.program.is_some(), "{:?}", compiled.diagnostics);
    let conventions = compiled.diagnostics.iter().filter(|found| found.code.category() == Category::Convention);
    let mut found: Vec<Found> = conventions
        .map(|found| {
            let edits = found.fixes.iter().flat_map(|fix| &fix.edits).map(|edit| {
                let text = &compiled.files.get(edit.file).unwrap().text;
                (text[edit.range.clone()].to_owned(), edit.new_text.clone())
            });
            let mut edits: Vec<_> = edits.collect();
            edits.sort();
            (found.code, found.message.clone(), edits)
        })
        .collect();
    found.sort();
    found
}

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

fn renamed(from: &str, to: &str) -> (String, String) {
    (from.to_owned(), to.to_owned())
}

#[test]
fn a_test_id_that_is_not_camel_case_is_renamed_from_its_name_or_itself() {
    let tests = concat!(
        "tests:\n",
        "  - id: ReadsUser\n    name: Reads the user\n    call: users#get\n",
        "  - id: Lists_all\n    call: users#get\n",
        "  - id: readsAll\n    call: users#get\n",
        "  - id: ReadsAll\n    call: users#get\n",
    );
    let found = found(&[CONFIG, USERS, ("services/users/crud.test.yaml", tests)]);
    let warning = |id: &str, edits| (Code::V4001, format!("test ID `{id}` is not camelCase"), edits);
    assert_eq!(
        found,
        [
            warning("Lists_all", vec![renamed("Lists_all", "listsAll")]),
            warning("ReadsAll", vec![]),
            warning("ReadsUser", vec![renamed("ReadsUser", "readsTheUser")]),
        ]
    );
}

#[test]
fn a_test_id_that_is_not_a_third_person_verb_is_renamed_from_its_name_or_conjugated() {
    let tests = concat!(
        "tests:\n",
        "  - id: getUser\n    name: Gets the user\n    call: users#get\n",
        "  - id: userTest\n    call: users#get\n",
        "  - id: get_all\n    call: users#get\n",
        "  - id: getsUser\n    call: users#get\n",
    );
    let compiled = crate::compile(CompileInput {
        files: [CONFIG, USERS, ("services/users/crud.test.yaml", tests)]
            .iter()
            .map(|(path, text)| file(path, text))
            .collect(),
        ..CompileInput::default()
    });
    let warnings: Vec<(&str, Option<bool>)> = compiled
        .diagnostics
        .iter()
        .filter(|found| found.code == Code::V4001)
        .map(|found| (found.message.as_str(), found.fixes.first().map(|fix| fix.certain)))
        .collect();
    let mut warnings = warnings;
    warnings.sort();
    assert_eq!(
        warnings,
        [
            ("test ID `getUser` does not start with a third-person verb", Some(true)),
            ("test ID `get_all` is not camelCase", Some(false)),
            ("test ID `userTest` does not start with a third-person verb", Some(false)),
        ]
    );
    let found = found(&[CONFIG, USERS, ("services/users/crud.test.yaml", tests)]);
    let edits: Vec<_> = found.into_iter().flat_map(|(_, _, edits)| edits).collect();
    assert_eq!(
        edits,
        [renamed("getUser", "getsTheUser"), renamed("get_all", "getsAll"), renamed("userTest", "usersTest")]
    );
}

#[test]
fn an_api_id_is_renamed_with_every_reference_to_it() {
    let apis = concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: GetUser\n    path: /users/1\n",
        "  - id: make\n    method: POST\n    path: /users\n    cleanup:\n      call: Remove\n",
        "  - id: Remove\n    method: DELETE\n    path: /users/1\n",
    );
    let tests = "tests:\n  - id: readsUser\n    call: users#GetUser\n  - id: removes\n    call: 'users#Remove'\n";
    let found = found(&[CONFIG, (USERS.0, apis), ("services/users/crud.test.yaml", tests)]);
    let warning = |id: &str, edits| (Code::V4002, format!("api ID `{id}` is not camelCase"), edits);
    let get = vec![renamed("GetUser", "getUser"), renamed("users#GetUser", "users#getUser")];
    let remove =
        vec![renamed("'users#Remove'", "'users#remove'"), renamed("Remove", "remove"), renamed("Remove", "remove")];
    assert_eq!(found, [warning("GetUser", get), warning("Remove", remove)]);
}

#[test]
fn a_file_id_tag_or_profile_that_breaks_the_conventions_is_reported() {
    let config = CONFIG.1.replace("id: dev", "id: Dev");
    let tests = "tags: [SlowNetwork]\ntests:\n  - id: readsUser\n    call: users#get\n    tags: ['Big Data', smoke]\n";
    let found = found(&[(CONFIG.0, &config), USERS, ("services/users/CRUD.test.yaml", tests)]);
    let messages: Vec<(Code, &str, usize)> =
        found.iter().map(|(code, message, edits)| (*code, message.as_str(), edits.len())).collect();
    assert_eq!(
        messages,
        [
            (Code::V4003, "the file ID `users.CRUD` is not lower case", 0),
            (Code::V4004, "profile `Dev` is not a lower-case word", 0),
            (Code::V4004, "tag `Big Data` is not lower kebab-case", 0),
            (Code::V4004, "tag `SlowNetwork` is not lower kebab-case", 1),
        ]
    );
    assert_eq!(found[3].2, [renamed("SlowNetwork", "slow-network")]);
}

#[test]
fn a_test_file_named_after_its_service_is_reported() {
    let tests = "tests:\n  - id: readsUser\n    call: users#get\n";
    let found = found(&[CONFIG, USERS, ("services/users/users.test.yaml", tests)]);
    let message = "the file ID `users.users` repeats the service name `users`".to_owned();
    assert_eq!(found, [(Code::V4003, message, Vec::new())]);
}

#[test]
fn a_base_url_and_path_that_do_not_meet_at_one_slash_are_fixed() {
    let api = concat!(
        "baseUrl: ${var.baseUrl}/\n",
        "apis:\n",
        "  - id: get\n    path: users/1\n",
        "  - id: quoted\n    path: 'users'\n",
        "  - id: full\n    path: http://other/users\n",
        "  - id: templated\n    path: ${var.baseUrl}/users\n",
        "  - id: rooted\n    path: /users\n",
    );
    let found = found(&[CONFIG, ("services/users/users.api.yaml", api)]);
    let base = (Code::V4005, "`baseUrl` ends in `/`".to_owned(), vec![renamed("${var.baseUrl}/", "${var.baseUrl}")]);
    let path = |edits| (Code::V4005, "`path` does not start with `/`".to_owned(), edits);
    assert_eq!(found, [base, path(Vec::new()), path(vec![renamed("users/1", "/users/1")])]);
}

#[test]
fn a_braced_path_placeholder_is_a_warning_with_the_var_suggested() {
    let api = concat!(
        "baseUrl: ${var.baseUrl}\n",
        "apis:\n",
        "  - id: poll\n    path: /poll/{jobId}/runs/{runId}\n",
        "  - id: quoted\n    path: '/poll/{jobId}'\n",
        "  - id: templated\n    path: /poll/${var.baseUrl}/{ x }\n",
    );
    let found = found(&[CONFIG, ("services/jobs/jobs.api.yaml", api)]);
    let placeholder = |edits| (Code::V4006, "`{jobId}` in a `path` is sent as is; it is not a var".to_owned(), edits);
    let fixed = renamed("/poll/{jobId}/runs/{runId}", "/poll/${var.jobId}/runs/${var.runId}");
    assert_eq!(found, [placeholder(Vec::new()), placeholder(vec![fixed])]);
}

#[test]
fn a_path_value_ending_in_url_encode_is_a_warning_with_the_pipe_removed() {
    let api = concat!(
        "baseUrl: ${var.baseUrl}\n",
        "vars: {userId: '1', term: a b}\n",
        "apis:\n",
        "  - id: get\n    path: /users/${var.userId | urlEncode}\n",
        "  - id: quoted\n    path: '/search/${var.term | lower |urlEncode}/all'\n",
        "  - id: before\n    path: /users/${var.userId | urlEncode | upper}\n",
        "  - id: plain\n    path: /users/${var.userId}\n",
    );
    let found = found(&[CONFIG, ("services/users/users.api.yaml", api)]);
    let message = "`urlEncode` is not needed in a `path`: a run encodes each value there".to_owned();
    let removed = |text: &str| (Code::V4007, message.clone(), vec![renamed(text, "")]);
    assert_eq!(found, [removed(" | urlEncode"), removed(" |urlEncode")]);
}
