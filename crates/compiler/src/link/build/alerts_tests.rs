use std::sync::Arc;

use vc_model::{Alert, AlertChannel, AlertTrigger, ContentHash, Program, SourceFile, SourcePath, Tag, TemplateValue};

use crate::CompileInput;

const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nprofiles:\n  - id: staging\n    default: true\nplugins:\n  - id: acmeAuth\n    source: npm:@acme/vibranium-auth@2\n    config: {realm: partners, retries: 2}\n  - id: localReport\n    source: path:plugins/report\nalerts:\n  - on: [failure, recovery]\n    profiles: [staging, prod]\n    tags: [smoke]\n    notify:\n      - webhook: {url: https://hooks.example/vibranium}\n      - pagerduty: {routingKey: '${secret.PD_KEY}'}\n  - notify:\n      - slack: {channel: '#qa', webhook: '${secret.SLACK_HOOK}'}\n";

const SCHEMA: &str = "# @sensitive\nPD_KEY=\nSLACK_HOOK=\n";

fn program(config: &str) -> Arc<Program> {
    let file = |path: &str, text: &str| SourceFile {
        path: SourcePath::new(path).unwrap(),
        text: text.to_owned(),
        hash: ContentHash::new([0; 32]),
    };
    let files = vec![file("vibranium.config.yaml", config)];
    let secret_schema = Some(file(".env.schema", SCHEMA));
    let compiled = crate::compile(CompileInput { files, secret_schema, ..CompileInput::default() });
    assert!(!compiled.diagnostics.has_errors(), "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

fn text(value: &TemplateValue) -> String {
    serde_json::to_string(value).unwrap()
}

#[test]
fn lowers_each_alert_with_its_triggers_filters_and_channels() {
    let program = program(CONFIG);
    let [first, second] = program.config.alerts.as_slice() else { panic!("{:?}", program.config.alerts) };
    assert_eq!(first.on, [AlertTrigger::Failure, AlertTrigger::Recovery]);
    assert_eq!(first.profiles, ["staging", "prod"]);
    assert_eq!(first.tags, [Tag("smoke".to_owned())]);
    let kinds = first.notify.iter().map(|channel| channel.kind.as_str()).collect::<Vec<_>>();
    assert_eq!(kinds, ["webhook", "pagerduty"]);
    assert!(text(&first.notify[0].settings).contains("https://hooks.example/vibranium"));
    assert!(text(&first.notify[1].settings).contains("PD_KEY"), "a secret stays a reference until the run ends");
    lowers_an_alert_with_the_defaults(second);
}

fn lowers_an_alert_with_the_defaults(second: &Alert) {
    assert_eq!(second.on, [AlertTrigger::Failure], "an alert that writes no `on` fires on failure");
    assert!(second.profiles.is_empty() && second.tags.is_empty());
    let [AlertChannel { kind, settings }] = second.notify.as_slice() else { panic!("{:?}", second.notify) };
    assert_eq!(kind, "slack");
    assert!(matches!(settings, TemplateValue::Map(map) if map.contains_key("channel") && map.contains_key("webhook")));
}

#[test]
fn lowers_each_plugin_with_its_source_and_config() {
    let program = program(CONFIG);
    let plugins = &program.config.plugins;
    let ids = plugins.iter().map(|plugin| program.str(plugin.id).unwrap_or_default()).collect::<Vec<_>>();
    assert_eq!(ids, ["acmeAuth", "localReport"]);
    assert_eq!(plugins[0].source, "npm:@acme/vibranium-auth@2");
    assert!(plugins[0].config.as_ref().is_some_and(|config| text(config).contains("partners")));
    assert_eq!((plugins[1].source.as_str(), &plugins[1].config), ("path:plugins/report", &None));
}

#[test]
fn a_config_without_alerts_or_plugins_has_none() {
    let program = program("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n");
    assert!(program.config.alerts.is_empty() && program.config.plugins.is_empty());
}
