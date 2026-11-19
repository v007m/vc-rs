use vc_diagnostics::Code;
use vc_model::{ContentHash, SourceFile, SourcePath};

use crate::{CompileInput, Compiled};

const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// The users apis with one `graphql` api whose `body` is `body`.
fn compile(body: &str) -> Compiled {
    let apis = format!("type: graphql\nbaseUrl: ${{var.baseUrl}}\napis:\n  - id: me\n    body:\n{body}");
    let tests = "tests:\n  - id: getsMe\n    call: users#me\n";
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", &apis),
        file("services/users/crud.test.yaml", tests),
    ];
    crate::compile(CompileInput { files, ..CompileInput::default() })
}

/// The code, message and line of each diagnostic `body` gives.
fn found(body: &str) -> Vec<(Code, String, u32)> {
    let compiled = compile(body);
    compiled
        .diagnostics
        .iter()
        .map(|found| (found.code, found.message.clone(), found.primary.span.start.line))
        .collect()
}

#[test]
fn a_graphql_body_takes_its_three_keys() {
    let compiled =
        compile("      query: 'query GetMe { me { id } }'\n      variables: {id: 1}\n      operationName: GetMe\n");
    assert!(compiled.diagnostics.is_empty(), "{:?}", compiled.diagnostics);
    assert!(compiled.program.is_some());
}

#[test]
fn a_misspelt_body_key_is_v2213_with_the_close_key_as_its_fix() {
    let compiled = compile("      query: '{ me { id } }'\n      varibles: {id: 1}\n      extensions: {}\n");
    let found: Vec<_> = compiled.diagnostics.iter().collect();
    let [misspelt, other] = found.as_slice() else { panic!("{:?}", compiled.diagnostics) };
    assert_eq!(
        (misspelt.code, misspelt.message.as_str()),
        (Code::V2213, "`varibles` does not apply to a `graphql` api's `body`")
    );
    assert_eq!(misspelt.primary.span.start.line, 7);
    assert_eq!(misspelt.fixes[0].title, "Replace with variables");
    assert!(misspelt.fixes[0].certain);
    assert_eq!((other.code, other.primary.span.start.line), (Code::V2213, 8));
    assert!(other.fixes.is_empty(), "no key is close to `extensions`");
    assert!(compiled.program.is_none());
}

#[test]
fn a_query_that_is_not_text_is_v2302() {
    for (query, kind) in [("42", "an integer"), ("null", "null"), ("[a]", "a list")] {
        let message = format!("`query` takes a GraphQL document as text, found {kind}");
        assert_eq!(found(&format!("      query: {query}\n")), [(Code::V2302, message, 6)], "{query}");
    }
}

#[test]
fn a_file_query_is_refused_until_it_is_read() {
    let message = "a `graphql` api cannot read its `query` from a `$file` yet".to_owned();
    assert_eq!(found("      query: {$file: me.graphql}\n"), [(Code::V2302, message, 6)]);
}

#[test]
fn an_operation_name_is_read_as_written() {
    let template = "`operationName` takes the operation's name as written, not a template".to_owned();
    let body = "      query: 'query GetMe { me { id } }'\n      operationName: ${var.op}\n";
    assert_eq!(found(body), [(Code::V2302, template, 7)]);
    let number = "`operationName` takes the operation's name as text, found an integer".to_owned();
    assert_eq!(found("      query: '{ me { id } }'\n      operationName: 7\n"), [(Code::V2302, number, 7)]);
}

#[test]
fn an_http_api_body_is_not_a_graphql_body() {
    let apis = "baseUrl: http://x\napis:\n  - id: make\n    method: POST\n    path: /users\n    body: {varibles: 1, query: 42}\n";
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", apis),
        file("services/users/crud.test.yaml", "tests:\n  - id: makesUser\n    call: users#make\n"),
    ];
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    assert!(compiled.diagnostics.is_empty(), "{:?}", compiled.diagnostics);
}
