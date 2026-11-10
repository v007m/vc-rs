use std::sync::Arc;

use vc_executor::MASKED;
use vc_testkit::{FakeCommands, FakeVcs, MemoryEnvironment, MemorySources};

use super::ConfigValues;
use crate::sample::builder;
use crate::{Engine, EngineError, ExplainRequest};

const SECRET: &str = "tok-fixture-9f3a71";
const NAMED: &str = "plain-but-named-42";
const SCHEMA: (&str, &[u8]) = (".env.schema", b"SHOP_TOKEN=\n");
const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nvars:\n  note: 'sent ${secret.SHOP_TOKEN}'\n  region: us\nprofiles:\n  - id: dev\n    default: true\n",
);
const VARLOCK: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nsecrets: {provider: varlock}\nvars:\n  note: 'sent ${secret.SHOP_TOKEN}'\n  region: us\nprofiles:\n  - id: dev\n    default: true\n",
);

fn project(config: (&str, &'static [u8])) -> Engine {
    project_with(config, FakeCommands::new(|_| None))
}

fn project_with(config: (&str, &'static [u8]), commands: FakeCommands) -> Engine {
    let environment = [("SHOP_TOKEN", SECRET), ("VIB_VAR_note", "hello"), ("VIB_VAR_SHOP_TOKEN", NAMED)];
    builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[config, SCHEMA])))
        .environment(Arc::new(MemoryEnvironment::new(&environment)))
        .commands(Arc::new(commands))
        .build()
        .unwrap()
}

async fn values(engine: &Engine, keys: &[&str], sets: &[&str]) -> ConfigValues {
    let keys: Vec<String> = keys.iter().map(|key| (*key).to_owned()).collect();
    let request =
        ExplainRequest { sets: sets.iter().map(|set| (*set).to_owned()).collect(), ..ExplainRequest::default() };
    engine.config_values(&keys, &request).await.unwrap()
}

fn layers(values: &ConfigValues, key: &str) -> Vec<(String, String)> {
    let value = values.values.iter().find(|value| value.key == key).unwrap();
    value.layers.iter().map(|layer| (layer.value.clone(), layer.source.clone())).collect()
}

#[tokio::test]
async fn a_value_from_a_secret_leaves_masked_in_every_layer_and_a_var_named_like_a_secret_whole() {
    let engine = project(CONFIG);
    let found = values(&engine, &["vars.note", "vars.SHOP_TOKEN", "vars.region"], &["vars.region=eu"]).await;
    let shown = format!("{found:?}");
    assert!(!shown.contains(SECRET) && !shown.contains(NAMED), "{shown}");
    assert_eq!(
        layers(&found, "vars.note"),
        [("hello".to_owned(), "VIB_VAR_note".to_owned()), (MASKED.to_owned(), "vibranium.config.yaml".to_owned())]
    );
    assert_eq!(layers(&found, "vars.SHOP_TOKEN"), [(MASKED.to_owned(), "VIB_VAR_SHOP_TOKEN".to_owned())]);
    let region = layers(&found, "vars.region");
    assert_eq!(region, [("eu".to_owned(), "--set".to_owned()), ("us".to_owned(), "vibranium.config.yaml".to_owned())]);
}

#[tokio::test]
async fn no_keys_gives_every_setting_then_every_var_something_sets() {
    let found = values(&project(CONFIG), &[], &[]).await;
    let keys: Vec<&str> = found.values.iter().map(|value| value.key.as_str()).collect();
    assert_eq!(keys.first(), Some(&"profile"));
    assert!(keys.contains(&"defaults.timeout") && keys.contains(&"vars.region"), "{keys:?}");
    assert_eq!(layers(&found, "defaults.timeout"), [("30s".to_owned(), "built-in".to_owned())]);
    assert!(!format!("{found:?}").contains(SECRET));
}

#[tokio::test]
async fn an_unknown_key_is_refused_with_the_closest_ones() {
    let keys = ["defaults.timout".to_owned()];
    let error = project(CONFIG).config_values(&keys, &ExplainRequest::default()).await.unwrap_err();
    assert!(matches!(&error, EngineError::Selection { .. }), "{error:?}");
    assert_eq!(error.help().as_deref(), Some("did you mean `defaults.timeout` or `defaults.perHost`?"));
}

#[tokio::test]
async fn an_unavailable_secret_source_is_never_asked_and_plain_values_stay_plain() {
    let commands = FakeCommands::new(|_| None);
    let engine = project_with(VARLOCK, commands.clone());
    let found = values(&engine, &["vars.region", "vars.note", "defaults.timeout"], &[]).await;
    assert_eq!(commands.calls(), 0);
    assert_eq!(layers(&found, "vars.region"), [("us".to_owned(), "vibranium.config.yaml".to_owned())]);
    assert_eq!(layers(&found, "vars.note")[1], (MASKED.to_owned(), "vibranium.config.yaml".to_owned()));
    assert_eq!(layers(&found, "defaults.timeout"), [("30s".to_owned(), "built-in".to_owned())]);
    assert!(found.notices.is_empty(), "{:?}", found.notices);
    assert!(!format!("{found:?}").contains(SECRET));
}

#[tokio::test]
async fn config_explain_prints_a_secret_masked() {
    let report = project(CONFIG).config_explain("vars.note", &ExplainRequest::default()).await.unwrap();
    assert_eq!(report.text, format!("vars.note = hello  (VIB_VAR_note)\n  over {MASKED}  (vibranium.config.yaml)\n"));
}
