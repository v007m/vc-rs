use vc_diagnostics::{Code, Diagnostic};
use vc_model::{ContentHash, SourceFile, SourcePath};

use crate::CompileInput;

const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nprofiles:\n  - id: dev\n    default: true\n";

fn needs(apis: &str) -> Vec<Diagnostic> {
    let file = |path: &str, text: &str| SourceFile {
        path: SourcePath::new(path).unwrap(),
        text: text.to_owned(),
        hash: ContentHash::new([0; 32]),
    };
    let files = vec![file("vibranium.config.yaml", CONFIG), file("services/users/users.api.yaml", apis)];
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    compiled.diagnostics.iter().filter(|found| found.code == Code::V1005).cloned().collect()
}

#[test]
fn an_http_api_without_a_path_is_reported_as_the_shape_check_would() {
    let found = needs("baseUrl: http://x\napis:\n  - id: get\n");
    let [missing] = found.as_slice() else { panic!("{found:?}") };
    assert_eq!(missing.message, "an apiEntry needs `path`");
    assert_eq!((missing.primary.span.start.line, missing.primary.span.start.column), (3, 5));
    assert_eq!(missing.help, ["add `path: /users/${var.userId}`: Appended to `baseUrl`"]);
}

#[test]
fn a_graphql_api_needs_a_query_but_no_path() {
    let found = needs("baseUrl: http://x\ntype: graphql\napis:\n  - id: getUser\n    body: {variables: {}}\n");
    let [missing] = found.as_slice() else { panic!("{found:?}") };
    assert_eq!(missing.message, "a `graphql` api needs `body.query`");
    assert_eq!(missing.primary.span.start.line, 4);
    assert_eq!(missing.help, ["add `body.query`: a `graphql` api cannot be sent without it"]);
    let written = "baseUrl: http://x\napis:\n  - id: getUser\n    type: graphql\n    body: {query: '{ me { id } }'}\n";
    assert_eq!(needs(written), []);
}

#[test]
fn an_api_without_a_body_needs_one_only_when_it_is_graphql() {
    assert_eq!(needs("baseUrl: http://x\napis:\n  - id: get\n    path: /users\n"), []);
    let found = needs("baseUrl: http://x\napis:\n  - id: get\n    type: graphql\n");
    assert_eq!(
        found.iter().map(|found| found.message.as_str()).collect::<Vec<_>>(),
        ["a `graphql` api needs `body.query`"]
    );
}
