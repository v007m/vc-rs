use std::time::Duration;

use vc_interfaces::{ConfigEntry, ConfigSource, ConfigSourceError};

use super::{Named, Settings};
use crate::EngineError;

struct Layer(Vec<ConfigEntry>);

impl ConfigSource for Layer {
    fn entries(&self) -> Result<Vec<ConfigEntry>, ConfigSourceError> {
        Ok(self.0.clone())
    }
}

struct Broken;

impl ConfigSource for Broken {
    fn entries(&self) -> Result<Vec<ConfigEntry>, ConfigSourceError> {
        Err(ConfigSourceError::Malformed { origin: ".env".into(), entry: "oops".into() })
    }
}

fn layer(origin: &str, pairs: &[(&str, &str)]) -> Layer {
    Layer(pairs.iter().map(|(key, value)| ConfigEntry::new(*key, *value, origin)).collect())
}

#[test]
fn later_layers_win_and_the_flags_sit_below_set() {
    let env = layer(".env", &[("profile", "dev"), ("defaults.concurrency", "2")]);
    let set = layer("--set", &[("defaults.concurrency", "8"), ("vars.userId", "7")]);
    let flags = [
        ConfigEntry::new("profile", "staging, eu", "--profile"),
        ConfigEntry::new("defaults.concurrency", "4", "--concurrency"),
    ];
    let settings = Settings::resolve(&[&env, &set], &flags).unwrap();
    let profiles = Named { names: vec!["staging".into(), "eu".into()], origin: "--profile".into() };
    assert_eq!(settings.profiles, Some(profiles));
    assert_eq!(settings.concurrency, Some(8));
    assert_eq!(settings.vars, [("userId".to_owned(), "7".to_owned())]);
}

#[test]
fn every_key_reads_its_value() {
    let pairs = [
        ("defaults.perHost", "3"),
        ("defaults.timeout", "2s"),
        ("reports.dir", "out"),
        ("reports.formats", "ctrf,junit"),
        ("reports.html.layout", "single"),
    ];
    let settings = Settings::resolve(&[&layer("--set", &pairs)], &[]).unwrap();
    assert_eq!(settings.per_host, Some(3));
    assert_eq!(settings.timeout.map(vc_model::Duration::get), Some(Duration::from_secs(2)));
    assert_eq!(settings.reports_dir.as_deref(), Some("out"));
    assert_eq!(settings.formats.map(|formats| formats.names), Some(vec!["ctrf".into(), "junit".into()]));
    assert_eq!(settings.html_single_file, Some(true));
    assert_eq!(Settings::resolve(&[], &[]).unwrap(), Settings::default());
}

#[test]
fn an_unknown_key_or_a_bad_value_names_its_layer() {
    let error = Settings::resolve(&[&layer("--set", &[("colour", "red")])], &[]).unwrap_err();
    assert_eq!(error.to_string(), "--set: `colour`: not a setting");
    assert!(error.help().is_some_and(|help| help.contains("vars.<name>")), "{error:?}");
    for (key, value) in [
        ("defaults.concurrency", "0"),
        ("defaults.perHost", "x"),
        ("defaults.timeout", "soon"),
        ("reports.dir", " "),
        ("reports.html.layout", "x"),
    ] {
        let error = Settings::resolve(&[&layer("VIB_PROFILE", &[(key, value)])], &[]).unwrap_err();
        assert!(matches!(&error, EngineError::Setting { origin, .. } if origin == "VIB_PROFILE"), "{error:?}");
    }
}

#[test]
fn a_layer_that_cannot_be_read_stops_the_run() {
    let error = Settings::resolve(&[&Broken, &layer("--set", &[])], &[]).unwrap_err();
    assert_eq!(error.to_string(), ".env: `oops` is not key=value");
}
