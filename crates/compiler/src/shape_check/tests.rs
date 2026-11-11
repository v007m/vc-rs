use vc_diagnostics::{Code, Diagnostic};
use vc_model::{FileKind, SourceFileId};
use vc_parser::{Node, Syntax};

use super::{LeafTy, Walk};
use crate::shapes;
use crate::shapes::{Scope, Unit};

fn tree(text: &str) -> Node {
    vc_parser::parse(SourceFileId::new(0), Syntax::Yaml, text).0.unwrap()
}

fn walk(kind: FileKind, text: &str) -> Walk {
    super::check(&tree(text), shapes::root(kind))
}

fn only(walk: &Walk) -> &Diagnostic {
    assert_eq!(walk.diagnostics.len(), 1, "{:?}", walk.diagnostics);
    walk.diagnostics.iter().next().unwrap()
}

#[test]
fn reports_an_unknown_key_with_a_certain_fix_for_one_close_key() {
    let walk = walk(FileKind::Api, "nmae: users\n");
    let unknown = only(&walk);
    assert_eq!(unknown.code, Code::V1004);
    assert_eq!(unknown.message, "unknown key `nmae` in api");
    assert_eq!(unknown.help, ["did you mean `name`?"]);
    assert_eq!(unknown.fixes[0].edits[0].new_text, "name");
}

#[test]
fn lists_the_keys_when_nothing_is_close() {
    let walk = walk(FileKind::Load, "mix: []\nstages: []\nzzz: 1\n");
    let unknown = only(&walk);
    assert!(unknown.help[0].starts_with("expected one of `id`, `name`, `mix`"), "{:?}", unknown.help);
    assert_eq!(unknown.fixes, []);
}

#[test]
fn reports_missing_required_keys_at_the_first_key() {
    let walk = walk(FileKind::Api, "apis:\n  - id: get\n");
    let missing = only(&walk);
    assert_eq!(missing.code, Code::V1005);
    assert_eq!(missing.message, "an apiEntry needs `path`");
    assert_eq!(missing.primary.span.start.line, 2);
    assert!(missing.help[0].starts_with("add `path: /users/${var.userId}`"));
    let config = walk_config("project: x\n");
    assert_eq!(only(&config).code, Code::V1003);
}

fn walk_config(text: &str) -> Walk {
    walk(FileKind::Config, text)
}

#[test]
fn offers_to_unquote_a_quoted_number_or_flag() {
    let walk = walk_config("syntaxVersion: '2'\n");
    let wrong = only(&walk);
    assert_eq!((wrong.code, wrong.message.as_str()), (Code::V2302, "expected integer, found text"));
    assert_eq!(wrong.fixes[0].title, "Write 2 without quotes");
    let walk = walk_config("syntaxVersion: 2\ndefaults: {concurrency: x}\n");
    assert_eq!(only(&walk).fixes, []);
    let flag = walk_config("syntaxVersion: 2\nprofiles: [{id: dev, default: \"true\"}]\n");
    assert_eq!(only(&flag).fixes[0].edits[0].new_text, "true");
}

#[test]
fn checks_maps_lists_and_words() {
    let not_map = walk(FileKind::Api, "apis: [x]\n");
    assert_eq!(only(&not_map).message, "expected a map of `id`, `name`, `method`, …, found text");
    let not_list = walk(FileKind::Api, "apis: {}\n");
    assert_eq!(only(&not_list).message, "expected a list of maps of `id`, `name`, `method`, …, found a map");
    let word = walk(FileKind::Api, "apis: [{id: a, path: /, method: GTE}]\n");
    let word = only(&word);
    assert_eq!(word.message, "expected one of GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, found `GTE`");
    assert_eq!(word.help, ["did you mean `GET`?"]);
}

#[test]
fn needs_exactly_one_of_a_group() {
    let none = walk(FileKind::Test, "tests: [{id: getsUser}]\n");
    assert_eq!(
        (only(&none).code, only(&none).message.as_str()),
        (Code::V2201, "a testEntry needs one of `call`, `steps`")
    );
    let both = walk(FileKind::Test, "tests: [{id: getsUser, call: users#get, steps: []}]\n");
    let both = only(&both);
    assert_eq!(both.message, "write only one of `call`, `steps`");
    assert_eq!(both.secondary.len(), 1);
    assert!(both.primary.span.start.column > both.secondary[0].span.start.column);
}

#[test]
fn collects_leaves_with_their_scopes() {
    let text = "tests:\n  - id: a\n    call: users#get\n    examples: [{n: 1}]\n    expect:\n      status: 200\n      timing: {total: {lt: 1s}}\n      cel: {ok: 'true'}\n";
    let walk = walk(FileKind::Test, text);
    assert!(walk.diagnostics.is_empty(), "{:?}", walk.diagnostics);
    let kinds: Vec<_> = walk.leaves.iter().map(|leaf| leaf.ty).collect();
    assert_eq!(kinds, [LeafTy::Value, LeafTy::Status, LeafTy::Matcher(Unit::Duration), LeafTy::Cel]);
    let status = &walk.leaves[1];
    assert!(status.scope.has(Scope::CALL.with(Scope::EXAMPLES)));
    assert!(!walk.leaves[0].scope.has(Scope::EXAMPLES), "examples does not see itself");
    let tree = tree(text);
    assert_eq!(
        super::at(&tree, &status.path).and_then(Node::as_scalar).map(|scalar| scalar.text.as_str()),
        Some("200")
    );
    assert!(super::at(&tree, &[0, 0, 0, 0, 0]).is_none());
}

#[test]
fn reads_a_row_source_before_the_run_but_inline_rows_per_instance() {
    let scope = |examples: &str| {
        let text = format!("tests:\n  - id: a\n    call: users#get\n    examples: {examples}\n");
        walk(FileKind::Test, &text).leaves[0].scope
    };
    assert!(scope("'${dataset.cities | take(3)}'").has(Scope::ROWS));
    assert!(scope("{$file: rows.csv}").has(Scope::ROWS));
    assert!(!scope("[{n: 1}]").has(Scope::ROWS), "inline rows render per instance");
}

#[test]
fn records_the_ids_of_actions() {
    let text = "beforeAll:\n  - id: seed\n    command: ./seed\n  - command: ./other\n";
    assert_eq!(walk(FileKind::Test, text).steps, ["seed"]);
}

#[test]
fn takes_the_first_alternative_that_fits() {
    let walk = walk(FileKind::Api, "auth: {id: shopOauth, vars: {a: 1}}\napis: [{id: a, path: /, auth: none}]\n");
    assert!(walk.diagnostics.is_empty(), "{:?}", walk.diagnostics);
    let wrong = super::check(&tree("auth: [x]\n"), shapes::root(FileKind::Api));
    assert_eq!(only(&wrong).message, "expected reference or authUse, found a list");
}

#[test]
fn checks_inside_the_alternative_it_takes() {
    let walk = walk(FileKind::Api, "auth: {id: shopOauth, varz: {a: 1}}\n");
    assert_eq!(only(&walk).message, "unknown key `varz` in authUse");
    let id = walk_config("syntaxVersion: 2\nprofiles: [{id: 5}]\n");
    assert_eq!(only(&id).code, Code::V2302);
}

#[test]
fn takes_a_schema_as_a_map_or_a_file_name() {
    let body =
        |schema: &str| format!("tests:\n  - id: a\n    call: users#get\n    expect: {{body: {{schema: {schema}}}}}\n");
    assert!(walk(FileKind::Test, &body("{$file: user.json}")).diagnostics.is_empty());
    assert!(walk(FileKind::Test, &body("user.json")).diagnostics.is_empty());
    assert_eq!(only(&walk(FileKind::Test, &body("[x]"))).code, Code::V2302);
}

#[test]
fn adds_sibling_roots_only_when_the_sibling_is_written() {
    let walk = walk(FileKind::Test, "tests:\n  - id: a\n    call: users#get\n    expect: {status: 200}\n");
    assert!(!walk.leaves[0].scope.has(Scope::EXAMPLES), "{:?}", walk.leaves);
    let sizes = walk_config("syntaxVersion: 2\nhistory: {maxBodySize: 1mb}\n");
    assert_eq!(sizes.leaves.iter().map(|leaf| leaf.ty).collect::<Vec<_>>(), [LeafTy::Size]);
}
