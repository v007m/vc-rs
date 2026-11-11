use std::sync::Arc;

use vc_diagnostics::{Category, Code};
use vc_model::{
    AgentPolicy, ContentHash, Matcher, MatcherArg, Method, Program, SourceFile, SourcePath, TemplateValue,
    WriteDefinitions,
};

use crate::{CompileInput, Compiled};

const CONFIG: &str = "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n    agents:\n      run: true\n      call: [GET, POST]\n      approve: [PUT, DELETE]\n      writeDefinitions: ask\n  - id: prod\n    agents: {writeDefinitions: true}\n  - id: local\n";

const APIS: &str = "baseUrl: ${var.baseUrl}\nsource:\n  openapi: specs/users.yaml\n  headers: {x-key: '${var.baseUrl}'}\n  include: [users]\n  only: [listUsers]\napis:\n  - id: list\n    path: /users\n    operationId: listUsers\n  - id: get\n    path: /users/1\n";

const TESTS: &str = "tests:\n  - id: listsUsers\n    call: users#list\n  - id: getsUser\n    call: users#get\n";

const LOAD: &str = "name: Peak\nmix:\n  - test: users.crud#listsUsers\n    weight: 3\n    thresholds: {p95: {lt: 200ms}}\n  - {test: users.crud#getsUser, weight: 1}\nstages:\n  - {duration: 1m, users: 10}\n  - {duration: 30s, users: 0}\nthinkTime: {min: 1s, max: 3s}\nchecks: full\nthresholds: {errorRate: {lt: 0.01}, p99: {lt: 1s}}\nabortOn: [errorRate]\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

fn compile(load: &str) -> Compiled {
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", APIS),
        file("services/users/crud.test.yaml", TESTS),
        file("load/peak.load.yaml", load),
    ];
    crate::compile(CompileInput { files, ..CompileInput::default() })
}

fn build(load: &str) -> Arc<Program> {
    let compiled = compile(load);
    let lint = |found: &vc_diagnostics::Diagnostic| found.code.category() == Category::Lifecycle;
    assert!(compiled.diagnostics.iter().all(lint), "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

fn seconds(duration: vc_model::Duration) -> u64 {
    duration.get().as_secs()
}

#[test]
fn builds_a_load_file_into_a_scenario() {
    let program = build(LOAD);
    let [scenario] = &program.loads.values().collect::<Vec<_>>()[..] else { panic!("one scenario") };
    assert_eq!((program.str(scenario.id.sym()), scenario.name.as_deref()), (Some("peak"), Some("Peak")));
    let mix: Vec<_> = scenario.mix.iter().map(|entry| (program.str(entry.test.sym()), entry.weight)).collect();
    assert_eq!(mix, [(Some("users.crud#listsUsers"), 3), (Some("users.crud#getsUser"), 1)]);
    assert_eq!(scenario.mix[0].thresholds.keys().collect::<Vec<_>>(), ["p95"]);
    assert!(scenario.mix[1].thresholds.is_empty());
    let stages: Vec<_> = scenario.stages.iter().map(|stage| (seconds(stage.duration), stage.users)).collect();
    assert_eq!(stages, [(60, 10), (30, 0)]);
    assert_eq!((seconds(scenario.think_time.0), seconds(scenario.think_time.1)), (1, 3));
    assert!(scenario.full_checks);
    assert_eq!(scenario.thresholds.keys().collect::<Vec<_>>(), ["errorRate", "p99"]);
    assert_eq!(scenario.abort_on, ["errorRate"]);
}

#[test]
fn a_load_file_defaults_to_api_checks_and_no_pause() {
    let program = build("mix:\n  - {test: users.crud#getsUser, weight: 1}\nstages:\n  - {duration: 2s, users: 1}\n");
    let scenario = program.loads.values().next().unwrap();
    assert!(!scenario.full_checks && scenario.thresholds.is_empty() && scenario.abort_on.is_empty());
    assert_eq!((seconds(scenario.think_time.0), seconds(scenario.think_time.1)), (0, 0));
    let fixed = build(
        "mix:\n  - {test: users.crud#getsUser, weight: 1}\nstages:\n  - {duration: 2s, users: 1}\nthinkTime: 2s\n",
    );
    let scenario = fixed.loads.values().next().unwrap();
    assert_eq!((seconds(scenario.think_time.0), seconds(scenario.think_time.1)), (2, 2));
}

#[test]
fn an_unknown_mix_test_is_v2003() {
    let compiled = compile("mix:\n  - {test: users.crud#missing, weight: 1}\nstages: []\n");
    let codes: Vec<_> = compiled.diagnostics.iter().map(|found| found.code).collect();
    assert_eq!(codes, [Code::V2003]);
    assert!(compiled.program.is_none());
}

#[test]
fn an_error_rate_takes_a_percentage_or_a_share() {
    let rate = |threshold: &str| {
        let program = build(&format!(
            "mix:\n  - {{test: users.crud#getsUser, weight: 1}}\nstages:\n  - {{duration: 2s, users: 1}}\nthresholds: {{errorRate: {threshold}}}\n"
        ));
        let scenario = program.loads.values().next().unwrap();
        let Some(Matcher::All(calls)) = scenario.thresholds.get("errorRate") else { panic!("a matcher") };
        let MatcherArg::Value(TemplateValue::Number(share)) = &calls[0].arg else { panic!("a number") };
        share.clone()
    };
    let share = |value| serde_json::Number::from_f64(value).unwrap();
    assert_eq!([rate("{lt: 1%}"), rate("{lt: 0.01}"), rate("{gte: 100%}")], [share(0.01), share(0.01), share(1.0)]);
    let compiled =
        compile("mix:\n  - {test: users.crud#getsUser, weight: 1}\nstages: []\nthresholds: {errorRate: {lt: 150%}}\n");
    let codes: Vec<_> = compiled.diagnostics.iter().map(|found| found.code).collect();
    assert_eq!(codes, [Code::V2301]);
}

#[test]
fn builds_profile_agents_rules() {
    let program = build(LOAD);
    let policy =
        |id: &str| &program.profiles.values().find(|profile| program.str(profile.id.sym()) == Some(id)).unwrap().agents;
    let dev = policy("dev");
    assert!(dev.run);
    assert_eq!(
        (&dev.call[..], &dev.approve[..]),
        (&[Method::Get, Method::Post][..], &[Method::Put, Method::Delete][..])
    );
    assert_eq!(dev.write_definitions, WriteDefinitions::Ask);
    let prod = policy("prod");
    assert!(!prod.run && prod.call.is_empty());
    assert_eq!(prod.write_definitions, WriteDefinitions::Allow);
    assert_eq!(policy("local"), &AgentPolicy::default());
}

#[test]
fn builds_an_api_files_source_and_operation_ids() {
    let program = build(LOAD);
    let group = program.api_groups.values().next().unwrap();
    let source = group.source.as_ref().unwrap();
    assert_eq!((source.openapi.as_str(), &source.include[..]), ("specs/users.yaml", &["users".to_owned()][..]));
    assert_eq!(source.headers.keys().collect::<Vec<_>>(), ["x-key"]);
    assert_eq!(source.only, ["listUsers"]);
    let ids: Vec<_> = program.apis.values().map(|api| api.operation_id.as_deref()).collect();
    assert_eq!(ids, [Some("listUsers"), None]);
}
