use std::sync::Arc;

use vc_testkit::{FakeVcs, MemoryEnvironment, MemorySources};

use super::{SETTINGS, duration};
use crate::reporting::{FILES, REPORTS_DIR};
use crate::run::{CONCURRENCY, PER_HOST};
use crate::sample::{builder, engine};
use crate::{Engine, EngineError, ExplainRequest};

const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\ndefaults: {timeout: 20s, concurrency: 4}\nreports: {dir: out, formats: [ctrf, html], html: {layout: single}}\nprofiles:\n  - id: dev\n    default: true\n    vars: {baseUrl: 'http://dev'}\n  - id: qa\nvars:\n  baseUrl: http://x\n",
);

fn project(environment: &[(&str, &str)]) -> Engine {
    builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[CONFIG])))
        .environment(Arc::new(MemoryEnvironment::new(environment)))
        .build()
        .unwrap()
}

fn explained(engine: &Engine, key: &str, request: &ExplainRequest) -> String {
    engine.config_explain(key, request).unwrap().text
}

#[test]
fn a_setting_shows_its_value_over_the_ones_it_replaced() {
    let layered = project(&[("VIB_DEFAULTS_TIMEOUT", "45s")]);
    let request = ExplainRequest { sets: vec!["defaults.timeout=1m".into()], ..ExplainRequest::default() };
    let expected = "defaults.timeout = 1m  (--set)\n  over 45s  (VIB_DEFAULTS_TIMEOUT)\n  over 20s  (vibranium.config.yaml)\n  over 30s  (built-in)\n";
    assert_eq!(explained(&layered, "defaults.timeout", &request), expected);
    let plain = explained(&engine(FakeVcs::default()), "defaults.timeout", &ExplainRequest::default());
    assert_eq!(plain, "defaults.timeout = 30s  (built-in)\n");
}

#[test]
fn each_setting_reads_the_config() {
    let engine = project(&[]);
    let first =
        |key: &str| explained(&engine, key, &ExplainRequest::default()).lines().next().unwrap_or_default().to_owned();
    assert_eq!(first("profile"), "profile = dev  (vibranium.config.yaml)");
    assert_eq!(first("defaults.concurrency"), "defaults.concurrency = 4  (vibranium.config.yaml)");
    assert_eq!(first("defaults.perHost"), "defaults.perHost = 8  (built-in)");
    assert_eq!(first("reports.dir"), "reports.dir = out  (vibranium.config.yaml)");
    assert_eq!(first("reports.formats"), "reports.formats = ctrf,html  (vibranium.config.yaml)");
    assert_eq!(first("reports.html.layout"), "reports.html.layout = single  (vibranium.config.yaml)");
    let named = ExplainRequest { profiles: vec!["qa".into()], ..ExplainRequest::default() };
    assert_eq!(
        explained(&engine, "profile", &named),
        "profile = qa  (--profile)\n  over dev  (vibranium.config.yaml)\n"
    );
}

#[test]
fn a_var_shows_the_level_of_each_value() {
    let engine = project(&[("VIB_VAR_baseUrl", "http://env")]);
    let expected = "vars.baseUrl = http://env  (VIB_VAR_baseUrl)\n  over http://dev  (profile dev)\n  over http://x  (vibranium.config.yaml)\n";
    assert_eq!(explained(&engine, "vars.baseUrl", &ExplainRequest::default()), expected);
}

#[test]
fn an_unknown_key_is_refused_with_the_closest_ones() {
    let engine = project(&[]);
    let error = engine.config_explain("defaults.timout", &ExplainRequest::default()).unwrap_err();
    let help = Some("did you mean `defaults.timeout` or `defaults.perHost`?".to_owned());
    assert_eq!(error, EngineError::Selection { reason: "no setting `defaults.timout`".into(), help });
    let error = engine.config_explain("vars.baseUlr", &ExplainRequest::default()).unwrap_err();
    assert_eq!(error.help().as_deref(), Some("did you mean `vars.baseUrl`?"));
    let error = engine.config_explain("zzz", &ExplainRequest::default()).unwrap_err();
    assert!(error.help().is_some_and(|help| help.starts_with("the settings are profile, defaults.concurrency")));
}

#[test]
fn the_built_in_values_are_the_engines() {
    let built_in = |key: &str| SETTINGS.iter().find(|(name, _, _)| *name == key).and_then(|(_, value, _)| *value);
    assert_eq!(built_in("defaults.concurrency"), Some(CONCURRENCY.to_string().as_str()));
    assert_eq!(built_in("defaults.perHost"), Some(PER_HOST.to_string().as_str()));
    assert_eq!(built_in("reports.dir"), Some(REPORTS_DIR));
    assert_eq!(built_in("reports.formats"), Some(FILES.join(",").as_str()));
    assert_eq!(
        (duration(std::time::Duration::from_millis(250)), duration(std::time::Duration::from_secs(20))),
        ("250ms".to_owned(), "20s".to_owned())
    );
}
