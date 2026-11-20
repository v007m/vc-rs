use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Api, BodySpec, ContentHash, Method, Part, Program, Protocol, SourceFile, SourcePath};

use crate::{CompileInput, Compiled};

const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {apiUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

/// An `sse` file that reconnects: an api that inherits it, one that turns it off, and an `http`
/// api the file's block does not reach.
const SSE: &str = "\
baseUrl: ${var.apiUrl}
type: sse
sse:
  reconnect: true
apis:
  - id: prices
    path: /prices
  - id: search
    method: POST
    path: /search
    body: {text: shoes}
    sse: {reconnect: false}
  - id: health
    type: http
    path: /health
";

/// A `websocket` api and an `sse` api with no `sse` block, in a file of the default type.
const MIXED: &str = "\
baseUrl: ${var.apiUrl}
apis:
  - id: chat
    type: websocket
    path: /chat
    headers: {x-room: lobby}
  - id: ticks
    type: sse
    path: /ticks
";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

fn compile(apis: &str, tests: &str) -> Compiled {
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/feed/feed.api.yaml", apis),
        file("services/feed/live.test.yaml", tests),
    ];
    crate::compile(CompileInput { files, ..CompileInput::default() })
}

fn compiled(apis: &str) -> std::sync::Arc<Program> {
    let compiled = compile(apis, "tests: []\n");
    assert_eq!(compiled.diagnostics.iter().count(), 0, "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

fn api<'p>(program: &'p Program, id: &str) -> &'p Api {
    program.apis.values().find(|api| program.str(api.id.sym()) == Some(&format!("feed#{id}"))).unwrap()
}

fn path(api: &Api) -> &str {
    let [Part::Literal(path)] = api.path.parts.as_slice() else { panic!("{:?}", api.path) };
    path
}

/// The V2213 diagnostics of a compile, as `(message, line)`, in report order.
fn stray(compiled: &Compiled) -> Vec<(String, u32)> {
    let found = compiled.diagnostics.iter().filter(|found| found.code == Code::V2213);
    found.map(|found| (found.message.clone(), found.primary.span.start.line)).collect()
}

#[test]
fn an_sse_api_sends_its_request_as_http_and_reconnects_as_its_own_block_else_its_files_says() {
    let program = compiled(SSE);
    let (prices, search, health) = (api(&program, "prices"), api(&program, "search"), api(&program, "health"));
    assert_eq!((prices.protocol, prices.method, path(prices)), (Protocol::Sse, Method::Get, "/prices"));
    assert_eq!((prices.body.clone(), prices.reconnect), (BodySpec::None, true));
    assert_eq!((search.protocol, search.method, path(search)), (Protocol::Sse, Method::Post, "/search"));
    assert!(matches!(search.body, BodySpec::Json(_)), "{:?}", search.body);
    assert!(!search.reconnect, "the api's own `sse.reconnect: false` wins over its file's");
    assert_eq!((health.protocol, health.reconnect), (Protocol::Http, false), "only an `sse` api reconnects");
}

#[test]
fn an_sse_block_without_reconnect_leaves_the_files_and_none_at_all_is_off() {
    let program = compiled(&SSE.replace("    sse: {reconnect: false}\n", "    sse: {}\n"));
    assert!(api(&program, "search").reconnect, "an api `sse` block that does not set it keeps the file's");
    let program = compiled(MIXED);
    assert!(!api(&program, "ticks").reconnect);
    assert_eq!(api(&program, "ticks").protocol, Protocol::Sse);
}

#[test]
fn a_websocket_api_opens_with_a_get_and_no_body_to_its_own_path() {
    let program = compiled(MIXED);
    let chat = api(&program, "chat");
    assert_eq!((chat.protocol, chat.method, path(chat)), (Protocol::Websocket, Method::Get, "/chat"));
    assert_eq!((chat.body.clone(), chat.reconnect, chat.headers.len()), (BodySpec::None, false, 1));
}

#[test]
fn a_websocket_api_reports_method_body_and_an_sse_block_each_with_its_removal() {
    let apis = MIXED
        .replace("    path: /chat\n", "    method: POST\n    path: /chat\n    body: hi\n    sse: {reconnect: true}\n");
    let compiled = compile(&apis, "tests: []\n");
    let expected = ["method", "body", "sse"].map(|key| format!("`{key}` does not apply to a `websocket` api"));
    let found: Vec<_> = stray(&compiled).into_iter().map(|(message, _)| message).collect();
    assert_eq!(found, expected);
    let removed: Vec<_> =
        compiled.diagnostics.iter().map(|found| &apis[found.fixes[0].edits[0].range.clone()]).collect();
    assert_eq!(removed, ["    method: POST\n", "    body: hi\n", "    sse: {reconnect: true}\n"]);
}

#[test]
fn a_websocket_api_s_stray_body_is_still_read_whole_so_its_template_mistakes_show_with_the_v2213() {
    let apis = MIXED.replace("    path: /chat\n", "    path: /chat\n    body: {text: '${var.chatId | nope}'}\n");
    let compiled = compile(&apis, "tests: []\n");
    let codes: Vec<_> = compiled.diagnostics.iter().map(|found| found.code).collect();
    assert_eq!(codes, [Code::V2308, Code::V2213]);
}

#[test]
fn a_call_to_a_websocket_api_takes_no_body_and_neither_stream_type_polls() {
    let tests = "\
tests:
  - id: chats
    call: feed#chat
    body: hello
  - id: watches
    call: feed#ticks
    poll: {every: 1s, timeout: 10s, until: {status: 200}}
    body: {from: 1}
  - id: talks
    call: feed#chat
    headers: {x-a: '1'}
    expect: {status: 101}
";
    let compiled = compile(MIXED, tests);
    let expected = [("`body` does not apply to a `websocket` api", 4), ("`poll` does not apply to a `sse` api", 7)];
    let found: Vec<_> = stray(&compiled);
    assert_eq!(found, expected.map(|(message, line)| (message.to_owned(), line)));
}

#[test]
fn a_test_level_body_that_reaches_a_websocket_step_is_reported_at_the_test_without_a_fix() {
    let tests =
        "tests:\n  - id: talks\n    body: {a: 1}\n    steps:\n      - call: feed#ticks\n      - call: feed#chat\n";
    let compiled = compile(MIXED, tests);
    let found: Vec<&Diagnostic> = compiled.diagnostics.iter().collect();
    let [body] = found.as_slice() else { panic!("{:?}", compiled.diagnostics) };
    assert_eq!((body.code, body.message.as_str()), (Code::V2213, "`body` does not apply to a `websocket` api"));
    assert_eq!((body.primary.span.start.line, body.secondary[0].span.start.line), (3, 6));
    assert_eq!(body.fixes, []);
}

#[test]
fn errors_is_a_graphql_check_so_a_stream_api_reports_it() {
    let apis = MIXED.replace("    path: /ticks\n", "    path: /ticks\n    expect: {errors: none}\n");
    assert_eq!(stray(&compile(&apis, "tests: []\n")), [("`errors` does not apply to a `sse` api".to_owned(), 10)]);
}

#[test]
fn an_sse_block_is_read_only_on_an_sse_api_or_an_sse_file() {
    let apis =
        "baseUrl: http://x\nsse:\n  reconnect: true\napis:\n  - id: get\n    path: /a\n    sse: {reconnect: true}\n";
    let compiled = compile(apis, "tests: []\n");
    let expected = [("`sse` does not apply to a `http` file", 2), ("`sse` does not apply to a `http` api", 7)];
    assert_eq!(stray(&compiled), expected.map(|(message, line)| (message.to_owned(), line)));
    let block = compiled.diagnostics.iter().next().unwrap();
    assert_eq!(&apis[block.fixes[0].edits[0].range.clone()], "sse:\n  reconnect: true\n");
    let typed = apis.replace("baseUrl: http://x\n", "baseUrl: http://x\ntype: sse\n");
    assert_eq!(compile(&typed, "tests: []\n").diagnostics.iter().count(), 0);
}

#[test]
fn a_misspelt_sse_key_is_v1004_with_the_close_key_and_a_stream_api_without_a_path_is_v1005() {
    let compiled = compile(&SSE.replace("  reconnect: true\n", "  reconect: true\n"), "tests: []\n");
    let found: Vec<_> =
        compiled.diagnostics.iter().map(|found| (found.code, found.fixes[0].edits[0].new_text.as_str())).collect();
    assert_eq!(found, [(Code::V1004, "reconnect")]);
    let apis = "baseUrl: http://x\napis:\n  - id: chat\n    type: websocket\n  - id: ticks\n    type: sse\n";
    let compiled = compile(apis, "tests: []\n");
    let missing: Vec<_> = compiled.diagnostics.iter().map(|found| (found.code, found.message.clone())).collect();
    let needs = |kind| (Code::V1005, format!("a `{kind}` api needs `path`"));
    assert_eq!(missing, [needs("websocket"), needs("sse")]);
}
