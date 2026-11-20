use vc_model::{
    Action, ContentHash, ExchangeItem, ExchangeKind, Matcher, MatcherArg, Part, Program, Segment, SourceFile,
    SourcePath, TemplateValue, Test, TestBody,
};

use crate::{CompileInput, Compiled};

const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {apiUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

/// An `sse` api that waits for a price tick, and a `websocket` api that joins a room and waits
/// for its welcome and its history in any order.
const APIS: &str = "\
baseUrl: ${var.apiUrl}
apis:
  - id: prices
    type: sse
    path: /prices
    exchange:
      - receive:
          event: tick
          data:
            match:
              price: {gt: 0}
              items[*].sku: {every: {startsWith: SKU-}}
  - id: chat
    type: websocket
    path: /chat
    exchange:
      - send: {type: join, room: lobby}
      - receive:
          - {type: joined}
          - {type: history, data: {equals: []}}
";

/// A test whose `call` replaces the api's exchange whole, and a test whose steps inherit its
/// exchange and its `expect.messages` unless they write their own.
const TESTS: &str = "\
tests:
  - id: getsTicks
    call: feed#prices
    exchange:
      - receive: {event: closed}
    expect:
      messages: {some: {event: tick}, length: {gt: 1}}
  - id: chatsInTurn
    exchange:
      - send: hello
      - receive: {text: {startsWith: hi}}
    expect:
      messages: {none: {type: error}}
    steps:
      - call: feed#chat
      - call: feed#chat
        exchange:
          - receive: {code: 1000, reason: done}
        expect:
          messages: {every: {type: {in: [text, ping]}}}
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

fn compiled() -> std::sync::Arc<Program> {
    let compiled = compile(APIS, TESTS);
    assert_eq!(compiled.diagnostics.iter().count(), 0, "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

fn test<'p>(program: &'p Program, id: &str) -> &'p Test {
    program.tests.values().find(|test| program.str(test.id.sym()) == Some(&format!("feed.live#{id}"))).unwrap()
}

fn actions<'p>(program: &'p Program, test: &Test) -> Vec<&'p Action> {
    let ids = match &test.body {
        TestBody::Call(id) => vec![*id],
        TestBody::Steps(ids) => ids.clone(),
    };
    ids.iter().map(|id| &program.actions[usize::try_from(id.index()).unwrap()]).collect()
}

/// Each item as `send …`, `receive …` or `any […]`, its matchers in [`shown`] form.
fn items(items: &[ExchangeItem]) -> Vec<String> {
    let kind = |item: &ExchangeItem| match &item.kind {
        ExchangeKind::Send(value) => format!("send {}", value_of(value)),
        ExchangeKind::Receive(check) => format!("receive {}", shown(check)),
        ExchangeKind::Group(checks) => format!("any [{}]", checks.iter().map(shown).collect::<Vec<_>>().join(", ")),
    };
    items.iter().map(kind).collect()
}

/// A matcher as `name(arg)` calls, a literal as itself.
fn shown(matcher: &Matcher) -> String {
    match matcher {
        Matcher::Equals(value) => value_of(value),
        Matcher::All(calls) => {
            calls.iter().map(|call| format!("{}({})", call.name, arg(&call.arg))).collect::<Vec<_>>().join(" ")
        }
        other => format!("{other:?}"),
    }
}

fn arg(arg: &MatcherArg) -> String {
    let pairs = |pairs: Vec<(String, &Matcher)>| {
        pairs.into_iter().map(|(name, matcher)| format!("{name}: {}", shown(matcher))).collect::<Vec<_>>().join(", ")
    };
    match arg {
        MatcherArg::Value(value) => value_of(value),
        MatcherArg::Matcher(matcher) => shown(matcher),
        MatcherArg::Matchers(all) => all.iter().map(shown).collect::<Vec<_>>().join(", "),
        MatcherArg::Fields(fields) => pairs(fields.iter().map(|(name, matcher)| (name.clone(), matcher)).collect()),
        MatcherArg::Paths(paths) => {
            pairs(paths.iter().map(|(path, matcher)| (dotted(&path.segments), matcher)).collect())
        }
        other => format!("{other:?}"),
    }
}

fn dotted(segments: &[Segment]) -> String {
    let segment = |segment: &Segment| match segment {
        Segment::Key(key) => key.clone(),
        Segment::Index(index) => format!("[{index}]"),
        Segment::All => "[*]".to_owned(),
    };
    segments.iter().map(segment).collect::<Vec<_>>().join(".")
}

fn value_of(value: &TemplateValue) -> String {
    match value {
        TemplateValue::Number(number) => number.to_string(),
        TemplateValue::String(template) => match template.parts.as_slice() {
            [Part::Literal(text)] => text.clone(),
            parts => format!("{parts:?}"),
        },
        TemplateValue::List(values) => format!("[{}]", values.iter().map(value_of).collect::<Vec<_>>().join(", ")),
        TemplateValue::Map(map) => {
            let entries: Vec<String> = map.iter().map(|(key, value)| format!("{key}: {}", value_of(value))).collect();
            format!("{{{}}}", entries.join(", "))
        }
        other => format!("{other:?}"),
    }
}

/// The `messages` checks of `action`, shown, in merge order.
fn messages(action: &Action) -> Vec<String> {
    let checks = action.expect.checks.iter().filter(|check| check.kind == vc_model::CheckKind::Messages);
    checks.map(|check| shown(&check.matcher)).collect()
}

#[test]
fn an_api_exchange_lowers_each_receive_field_by_field_and_data_by_its_paths_or_whole() {
    let program = compiled();
    let api = |id: &str| program.apis.values().find(|api| program.str(api.id.sym()) == Some(id)).unwrap();
    assert_eq!(
        items(&api("feed#prices").exchange),
        ["receive fields(event: tick, data: paths(price: gt(0), items.[*].sku: every(startsWith(SKU-))))"]
    );
    assert_eq!(
        items(&api("feed#chat").exchange),
        ["send {type: join, room: lobby}", "any [fields(type: joined), fields(type: history, data: [])]",],
        "a `receive` list is one group, matched in any order"
    );
    assert_eq!(api("feed#prices").exchange[0].span.start.line, 7);
}

#[test]
fn a_call_replaces_the_apis_exchange_whole_and_checks_its_messages_as_a_list() {
    let program = compiled();
    let [call] = actions(&program, test(&program, "getsTicks"))[..] else { panic!() };
    assert_eq!(items(call.exchange.as_deref().unwrap()), ["receive fields(event: closed)"]);
    assert_eq!(messages(call), ["some(fields(event: tick)) length(gt(1))"]);
}

#[test]
fn steps_inherit_the_tests_exchange_and_messages_unless_they_write_their_own() {
    let program = compiled();
    let [inheriting, own] = actions(&program, test(&program, "chatsInTurn"))[..] else { panic!() };
    assert_eq!(items(inheriting.exchange.as_deref().unwrap()), ["send hello", "receive fields(text: startsWith(hi))"]);
    assert_eq!(messages(inheriting), ["none(fields(type: error))"]);
    assert_eq!(items(own.exchange.as_deref().unwrap()), ["receive fields(code: 1000, reason: done)"]);
    assert_eq!(messages(own), ["every(fields(type: in([text, ping])))"], "a step's own `messages` replaces the test's");
}

#[test]
fn a_call_that_writes_no_exchange_leaves_the_apis() {
    let compiled = compile(APIS, "tests:\n  - id: getsPrices\n    call: feed#prices\n");
    let program = compiled.program.unwrap();
    let [call] = actions(&program, test(&program, "getsPrices"))[..] else { panic!() };
    assert_eq!(call.exchange, None, "the engine runs the api's exchange");
}
