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
