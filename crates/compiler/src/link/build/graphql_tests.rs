use vc_diagnostics::{Code, Diagnostic};
use vc_model::{
    Api, BodySpec, CheckKind, ContentHash, GraphqlBody, Matcher, MatcherArg, Method, Part, Program, Protocol,
    SourceFile, SourcePath, TemplateValue,
};

use crate::{CompileInput, Compiled};

const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {apiUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

/// A `graphql` file: its own `graphql.path`, an api that sets its own `path`, and an `http` one.
const APIS: &str = "\
baseUrl: ${var.apiUrl}
type: graphql
graphql:
  path: /api/graphql
apis:
  - id: getUser
    body:
      query: 'query GetUser($id: ID!, $limit: Int) { user(id: $id) { id name } }'
      variables: {id: '${var.userId}', limit: 2}
      operationName: GetUser
  - id: admin
    path: /admin/graphql
    body:
      query: 'query Admin { me { id } }'
  - id: health
    type: http
    path: /health
";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// Tests that call the graphql apis, so the linker checks their templates from a call.
const TESTS: &str = "tests:\n  - id: getsUser\n    call: users#getUser\n  - id: getsMe\n    call: users#admin\n";

fn compile(apis: &str, config: &str) -> Compiled {
    let files = vec![
        file("vibranium.config.yaml", config),
        file("services/users/users.api.yaml", apis),
        file("services/users/crud.test.yaml", TESTS),
    ];
    crate::compile(CompileInput { files, ..CompileInput::default() })
}

fn compiled(apis: &str) -> std::sync::Arc<Program> {
    let compiled = compile(apis, &format!("{CONFIG}    vars: {{userId: u1}}\n"));
    assert_eq!(compiled.diagnostics.iter().count(), 0, "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

fn api<'p>(program: &'p Program, id: &str) -> &'p Api {
    program.apis.values().find(|api| program.str(api.id.sym()) == Some(&format!("users#{id}"))).unwrap()
}

fn path(api: &Api) -> &str {
    let [Part::Literal(path)] = api.path.parts.as_slice() else { panic!("{:?}", api.path) };
    path
}

fn body(api: &Api) -> &GraphqlBody {
    let BodySpec::Graphql(body) = &api.body else { panic!("{:?}", api.body) };
    body
}

#[test]
fn a_graphql_api_posts_its_query_variables_and_operation_name() {
    let program = compiled(APIS);
    let get = api(&program, "getUser");
    assert_eq!((get.protocol, get.method), (Protocol::Graphql, Method::Post));
    let body = body(get);
    assert_eq!(body.document, "query GetUser($id: ID!, $limit: Int) { user(id: $id) { id name } }");
    assert_eq!(body.operation_name.as_deref(), Some("GetUser"));
}

#[test]
fn the_variables_are_templates_and_values_of_their_own_type() {
    let program = compiled(APIS);
    let body = body(api(&program, "getUser"));
    let TemplateValue::Map(variables) = &body.variables else { panic!("{:?}", body.variables) };
    assert_eq!(variables.keys().collect::<Vec<_>>(), ["id", "limit"]);
    assert!(matches!(&variables["id"], TemplateValue::String(id) if id.single_ref().is_some()), "{variables:?}");
    assert!(matches!(&variables["limit"], TemplateValue::Number(limit) if limit.as_u64() == Some(2)));
}

#[test]
fn unset_variables_are_an_empty_map_and_an_unset_operation_name_is_none() {
    let program = compiled(APIS);
    let body = body(api(&program, "admin"));
    assert_eq!(body.variables, TemplateValue::Map(indexmap::IndexMap::new()));
    assert_eq!(body.operation_name, None);
}

#[test]
fn the_path_is_the_apis_own_else_the_files_graphql_path_else_slash_graphql() {
    let program = compiled(APIS);
    assert_eq!((path(api(&program, "getUser")), path(api(&program, "admin"))), ("/api/graphql", "/admin/graphql"));
    let bare = APIS.replace("graphql:\n  path: /api/graphql\n", "");
    assert_eq!(path(api(&compiled(&bare), "getUser")), "/graphql");
}

#[test]
fn an_http_api_in_a_graphql_file_stays_http() {
    let program = compiled(APIS);
    let health = api(&program, "health");
    assert_eq!((health.protocol, health.method, path(health)), (Protocol::Http, Method::Get, "/health"));
    assert_eq!(health.body, BodySpec::None);
}

#[test]
fn the_query_is_a_document_so_a_dollar_brace_in_it_is_kept_and_never_checked() {
    let query = "'query Search { search(text: \"${x} and ${var.nope}\") { id } }'";
    let apis = APIS.replace("'query Admin { me { id } }'", query);
    let program = compiled(&apis);
    assert_eq!(body(api(&program, "admin")).document, "query Search { search(text: \"${x} and ${var.nope}\") { id } }");
}

#[test]
fn an_unset_var_in_variables_is_v2101() {
    let compiled = compile(APIS, CONFIG);
    let unset: Vec<&Diagnostic> = compiled.diagnostics.iter().filter(|found| found.code == Code::V2101).collect();
    let [unset] = unset.as_slice() else { panic!("{:?}", compiled.diagnostics) };
    assert_eq!(&APIS[unset.primary.span.start.offset..unset.primary.span.end.offset], "${var.userId}");
}

#[test]
fn errors_none_and_any_are_an_errors_check_that_the_list_is_empty_or_not() {
    for (word, empty) in [("none", true), ("any", false)] {
        let apis = APIS.replace(
            "      operationName: GetUser\n",
            &format!("      operationName: GetUser\n    expect:\n      errors: {word}\n"),
        );
        let program = compiled(&apis);
        let [check] = api(&program, "getUser").expect.checks.as_slice() else { panic!("{word}") };
        assert_eq!((check.kind, check.target.as_ref()), (CheckKind::Errors, None), "{word}");
        let Matcher::All(calls) = &check.matcher else { panic!("{:?}", check.matcher) };
        let [call] = calls.as_slice() else { panic!("{calls:?}") };
        assert_eq!(
            (call.name.as_str(), &call.arg),
            ("empty", &MatcherArg::Value(TemplateValue::Bool(empty))),
            "{word}"
        );
    }
}
