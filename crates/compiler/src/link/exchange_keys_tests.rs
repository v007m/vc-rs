use vc_diagnostics::{Code, Diagnostic};
use vc_model::{ContentHash, SourceFile, SourcePath};

use crate::{CompileInput, Compiled};

const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {apiUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

/// An `sse` api, a `websocket` api and an `http` one, each with what `extra` adds to it.
fn apis(sse: &str, websocket: &str) -> String {
    format!(
        "baseUrl: ${{var.apiUrl}}\napis:\n  - id: ticks\n    type: sse\n    path: /ticks\n{sse}  - id: chat\n    type: \
         websocket\n    path: /chat\n{websocket}  - id: health\n    path: /health\n"
    )
}

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

/// Each diagnostic as its code, line, message and certain fix's title.
fn found(compiled: &Compiled) -> Vec<(Code, u32, String, Option<String>)> {
    let fix = |found: &Diagnostic| found.fixes.iter().find(|fix| fix.certain).map(|fix| fix.title.clone());
    let row = |found: &Diagnostic| (found.code, found.primary.span.start.line, found.message.clone(), fix(found));
    compiled.diagnostics.iter().map(row).collect()
}

fn row(code: Code, line: u32, message: &str, fix: Option<&str>) -> (Code, u32, String, Option<String>) {
    (code, line, message.to_owned(), fix.map(str::to_owned))
}

/// A test file whose one test calls `api` with `keys`.
fn calling(api: &str, keys: &str) -> String {
    format!("tests:\n  - id: callsIt\n    call: feed#{api}\n{keys}")
}

#[test]
fn a_send_on_an_sse_api_is_v2213_without_a_fix() {
    let compiled = compile(&apis("    exchange:\n      - send: {hello: 1}\n", ""), "tests: []\n");
    let message = "`send` does not apply to an `sse` api";
    assert_eq!(found(&compiled), [row(Code::V2213, 7, message, None)]);
    let help = &compiled.diagnostics.iter().next().unwrap().help;
    assert!(help[0].contains("call a `websocket` api to send messages"), "{help:?}");
}

#[test]
fn a_field_the_types_messages_lack_is_v1004_with_the_close_name_as_its_fix() {
    let sse = "    exchange:\n      - receive: {evnt: tick, data: {equals: 1}}\n";
    let websocket = "    exchange:\n      - receive:\n          - {type: joined}\n          - {retry: 3}\n";
    let compiled = compile(&apis(sse, websocket), "tests: []\n");
    assert_eq!(
        found(&compiled),
        [
            row(Code::V1004, 7, "an `sse` message has no field `evnt`", Some("Replace with event")),
            row(Code::V1004, 14, "a `websocket` message has no field `retry`", None),
        ]
    );
    let help = &compiled.diagnostics.iter().nth(1).unwrap().help;
    assert_eq!(help, &["expected one of `type`, `data`, `text`, `code`, `reason`"]);
}

#[test]
fn expect_messages_checks_each_message_check_under_every_some_and_none_through_not_and_all_of() {
    let keys = "    expect:\n      messages:\n        length: {gt: 1}\n        not: {some: {type: ping}}\n        allOf:\n          - {every: {event: tick}}\n          - {none: {code: 1}}\n";
    let compiled = compile(&apis("", ""), &calling("ticks", keys));
    assert_eq!(
        found(&compiled),
        [
            row(Code::V1004, 7, "an `sse` message has no field `type`", None),
            row(Code::V1004, 10, "an `sse` message has no field `code`", None),
        ]
    );
    let compiled = compile(&apis("", ""), &calling("chat", "    expect:\n      messages: {some: {event: tick}}\n"));
    assert_eq!(found(&compiled), [row(Code::V1004, 5, "a `websocket` message has no field `event`", None)]);
}

#[test]
fn a_calls_exchange_is_checked_against_the_type_it_reaches() {
    let keys = "    exchange:\n      - send: hi\n      - receive: {type: joined}\n";
    let compiled = compile(&apis("", ""), &calling("ticks", keys));
    assert_eq!(
        found(&compiled),
        [
            row(Code::V2213, 5, "`send` does not apply to an `sse` api", None),
            row(Code::V1004, 6, "an `sse` message has no field `type`", None),
        ]
    );
    assert_eq!(found(&compile(&apis("", ""), &calling("chat", keys))), []);
    let http = found(&compile(&apis("", ""), &calling("health", keys)));
    assert_eq!(http, [row(Code::V2213, 4, "`exchange` does not apply to an `http` api", Some("Remove `exchange`"))]);
}

/// A test whose steps inherit its `exchange` and `expect.messages`: an `sse` step, a
/// `websocket` one, and an `sse` step that writes both its own.
const DEFAULTS: &str = "\
tests:
  - id: chatsAndTicks
    exchange:
      - send: hi
      - receive: {kind: joined}
    expect:
      messages: {every: {reason: done}}
    steps:
      - call: feed#ticks
      - call: feed#chat
      - call: feed#ticks
        exchange:
          - receive: {event: tick}
        expect:
          messages: {some: {event: tick}}
";

#[test]
fn a_test_level_default_is_checked_once_against_each_type_a_step_that_inherits_it_reaches() {
    let compiled = compile(&apis("", ""), DEFAULTS);
    assert_eq!(
        found(&compiled),
        [
            row(Code::V2213, 4, "`send` does not apply to an `sse` api", None),
            row(Code::V1004, 5, "an `sse` message has no field `kind`", None),
            row(Code::V1004, 7, "an `sse` message has no field `reason`", None),
        ],
        "`kind`, which neither type has, is reported once; `reason` is a websocket field only"
    );
    let own = DEFAULTS.replace("      - call: feed#ticks\n      - call: feed#chat\n", "      - call: feed#chat\n");
    let own = own.replace("{kind: joined}", "{type: joined}");
    assert_eq!(found(&compile(&apis("", ""), &own)), [], "the sse step writes its own, so it inherits neither");
}

#[test]
fn a_message_check_that_is_no_map_of_fields_is_v2306_and_data_takes_match_or_equals() {
    let receive = |check: &str| apis(&format!("    exchange:\n      - receive: {check}\n"), "");
    let cases = [
        ("7", Code::V2306, "a message check is a map of the message's fields", None),
        ("{data: 7}", Code::V2302, "`data` takes `match` or `equals`, found an integer", None),
        ("{data: {matches: {id: 7}}}", Code::V1004, "`data` has no `matches`", Some("Replace with match")),
        ("{data: {match: [id]}}", Code::V2302, "`data.match` takes a map of paths, found a list", None),
    ];
    for (check, code, message, fix) in cases {
        assert_eq!(found(&compile(&receive(check), "tests: []\n")), [row(code, 7, message, fix)], "{check}");
    }
}

#[test]
fn messages_take_a_matcher_on_the_list_and_a_path_through_a_list_takes_list_matchers() {
    let messages = |value: &str| calling("ticks", &format!("    expect:\n      messages: {value}\n"));
    let on_the_list = "`messages` takes a matcher on the list of messages";
    assert_eq!(found(&compile(&apis("", ""), &messages("{event: tick}"))), [row(Code::V2306, 5, on_the_list, None)]);
    let not_a_map = "a message check is a map of the message's fields";
    assert_eq!(found(&compile(&apis("", ""), &messages("{some: tick}"))), [row(Code::V2306, 5, not_a_map, None)]);
    let scalar = "{some: {data: {match: {'items[*].id': {gt: 1}}}}}";
    let found = found(&compile(&apis("", ""), &messages(scalar)));
    assert_eq!(found, [row(Code::V2306, 5, "`gt` compares one value, but `items[*].id` is a list", None)]);
}

#[test]
fn an_exchange_item_writes_one_of_send_and_receive() {
    let both = apis("", "    exchange:\n      - {send: hi, receive: {type: joined}}\n");
    let found = found(&compile(&both, "tests: []\n"));
    assert_eq!(found, [row(Code::V2201, 10, "write only one of `send`, `receive`", None)]);
}
