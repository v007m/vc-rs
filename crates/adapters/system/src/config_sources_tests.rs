use std::sync::Arc;

use vc_interfaces::{ConfigEntry, ConfigSource, ConfigSourceError};
use vc_testkit::{MemoryEnvironment, config_source_suite};

use super::{DotEnv, SetFlags, VibVars};

fn pairs(vars: &[(String, String)]) -> Vec<(&str, &str)> {
    vars.iter().map(|(name, value)| (name.as_str(), value.as_str())).collect()
}

#[test]
fn a_dot_env_meets_the_config_source_contract() {
    config_source_suite(&DotEnv::parse("VIB_PROFILE=staging\nTOKEN=x\n").unwrap());
    config_source_suite(&DotEnv::default());
}

#[test]
fn a_dot_env_reads_names_values_comments_exports_and_quotes() {
    let text = "# a comment\n\nTOKEN=abc\nexport BASE_URL = http://x \nQUOTED=\"a b\"\nSINGLE='c'\nEMPTY=\n";
    let env = DotEnv::parse(text).unwrap();
    let expected = [("TOKEN", "abc"), ("BASE_URL", "http://x"), ("QUOTED", "a b"), ("SINGLE", "c"), ("EMPTY", "")];
    assert_eq!(pairs(env.vars()), expected);
}

#[test]
fn only_the_vib_lines_of_a_dot_env_are_settings() {
    let env = DotEnv::parse("TOKEN=abc\nVIB_PROFILE=staging\n").unwrap();
    assert_eq!(env.entries().unwrap(), [ConfigEntry::new("profile", "staging", ".env")]);
}

#[test]
fn a_dot_env_line_without_an_equals_sign_is_malformed() {
    let error = DotEnv::parse("TOKEN=abc\njust words\n").unwrap_err();
    assert_eq!(error, ConfigSourceError::Malformed { origin: ".env".into(), entry: "just words".into() });
    assert!(DotEnv::parse("=value").is_err(), "a name is required");
    assert!(DotEnv::parse("TWO WORDS=value").is_err(), "a name has no spaces");
}

#[test]
fn a_project_without_a_dot_env_has_an_empty_layer() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(DotEnv::read(dir.path()).unwrap(), DotEnv::default());
    std::fs::write(dir.path().join(".env"), "VIB_PROFILE=qa\n").unwrap();
    assert_eq!(DotEnv::read(dir.path()).unwrap().entries().unwrap(), [ConfigEntry::new("profile", "qa", ".env")]);
}

#[cfg(unix)]
#[test]
fn a_dot_env_that_is_a_link_is_not_followed() {
    let outside = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secrets"), "TOKEN=x\n").unwrap();
    std::os::unix::fs::symlink(outside.path().join("secrets"), dir.path().join(".env")).unwrap();
    let error = DotEnv::read(dir.path()).unwrap_err();
    assert!(matches!(error, ConfigSourceError::Unreadable { .. }), "{error}");
}

#[test]
fn vib_vars_meet_the_config_source_contract_and_read_vib_profile() {
    let environment = Arc::new(MemoryEnvironment::new(&[("VIB_PROFILE", "staging"), ("HOME", "/h")]));
    let vars = VibVars::new(environment);
    config_source_suite(&vars);
    assert_eq!(vars.entries().unwrap(), [ConfigEntry::new("profile", "staging", "VIB_PROFILE")]);
    assert_eq!(VibVars::new(Arc::new(MemoryEnvironment::new(&[]))).entries().unwrap(), []);
}

#[test]
fn set_flags_meet_the_config_source_contract_and_keep_their_order() {
    let flags = SetFlags::new(vec!["defaults.concurrency=4".into(), "vars.userId = 7".into()]);
    config_source_suite(&flags);
    let expected =
        [ConfigEntry::new("defaults.concurrency", "4", "--set"), ConfigEntry::new("vars.userId", "7", "--set")];
    assert_eq!(flags.entries().unwrap(), expected);
}

#[test]
fn a_set_flag_without_a_value_is_malformed() {
    let error = SetFlags::new(vec!["concurrency".into()]).entries().unwrap_err();
    assert_eq!(error.to_string(), "--set: `concurrency` is not key=value");
}

#[test]
fn every_setting_has_a_vib_name_and_vars_keep_their_case() {
    let environment = Arc::new(MemoryEnvironment::new(&[
        ("VIB_DEFAULTS_CONCURRENCY", "4"),
        ("VIB_DEFAULTS_PER_HOST", "2"),
        ("VIB_DEFAULTS_TIMEOUT", "3s"),
        ("VIB_REPORTS_DIR", "out"),
        ("VIB_REPORTS_FORMATS", "ctrf,junit"),
        ("VIB_VAR_userId", "7"),
        ("VIB_VAR_", "nameless"),
        ("VIB_UNKNOWN", "x"),
    ]));
    let expected = [
        ConfigEntry::new("defaults.concurrency", "4", "VIB_DEFAULTS_CONCURRENCY"),
        ConfigEntry::new("defaults.perHost", "2", "VIB_DEFAULTS_PER_HOST"),
        ConfigEntry::new("defaults.timeout", "3s", "VIB_DEFAULTS_TIMEOUT"),
        ConfigEntry::new("reports.dir", "out", "VIB_REPORTS_DIR"),
        ConfigEntry::new("reports.formats", "ctrf,junit", "VIB_REPORTS_FORMATS"),
        ConfigEntry::new("vars.userId", "7", "VIB_VAR_userId"),
    ];
    assert_eq!(VibVars::new(environment).entries().unwrap(), expected);
}

#[test]
fn a_dot_env_sets_vars_through_vib_var_names() {
    let env = DotEnv::parse("VIB_VAR_who=dotenv\nVIB_DEFAULTS_PER_HOST=3\n").unwrap();
    let expected = [ConfigEntry::new("vars.who", "dotenv", ".env"), ConfigEntry::new("defaults.perHost", "3", ".env")];
    assert_eq!(env.entries().unwrap(), expected);
}
