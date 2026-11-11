//! `vc config explain <key>` shows a config value and the layer that set it (D129): built-in,
//! user config, project config, profile, `.env`, `VIB_*` or `--set`.

use crate::{CONFIG, Project, said};

/// What `vc config explain <key>` prints, with `env` set, asserting it succeeded.
fn explain(project: &Project, key: &str, env: Option<(&str, &str)>) -> String {
    let mut vc = project.vc(&["config", "explain", key]);
    if let Some((name, value)) = env {
        vc.env(name, value);
    }
    let output = vc.output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn a_value_nobody_set_comes_from_the_built_in_defaults() {
    let project = Project::new(&[CONFIG]);
    let text = explain(&project, "defaults.timeout", None);
    assert!(text.contains("30s") && text.contains("built-in"), "{text}");
}

#[test]
fn a_value_the_project_sets_names_the_config_file() {
    let config = format!("{}defaults:\n  timeout: 20s\n", CONFIG.1);
    let project = Project::new(&[(CONFIG.0, &config)]);
    let text = explain(&project, "defaults.timeout", None);
    assert!(text.contains("20s") && text.contains("vibranium.config.yaml"), "{text}");
}

#[test]
fn an_environment_variable_wins_and_is_named() {
    let config = format!("{}defaults:\n  timeout: 20s\n", CONFIG.1);
    let project = Project::new(&[(CONFIG.0, &config)]);
    let text = explain(&project, "defaults.timeout", Some(("VIB_DEFAULTS_TIMEOUT", "45s")));
    assert!(text.contains("45s") && text.contains("VIB_DEFAULTS_TIMEOUT"), "{text}");
}

#[test]
fn a_profile_var_names_its_profile() {
    let config = "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n    vars: {baseUrl: 'http://dev'}\n";
    let project = Project::new(&[(CONFIG.0, config)]);
    let text = explain(&project, "vars.baseUrl", None);
    assert!(text.contains("http://dev") && text.contains("dev"), "{text}");
    assert!(text.contains("http://x"), "the value it overrides is shown too:\n{text}");
}

/// `defaults.retry` names its two settings; `--set` overrides one and leaves the other.
#[test]
fn the_retry_group_shows_its_attempts_and_delay_and_set_overrides_one() {
    let config = format!("{}defaults:\n  retry: {{attempts: 1}}\n", CONFIG.1);
    let project = Project::new(&[(CONFIG.0, &config)]);
    let expected = "defaults.retry.attempts = 1  (vibranium.config.yaml)\n  over 0  (built-in)\n\
                    defaults.retry.delay = 0s  (vibranium.config.yaml)\n  over 0s  (built-in)\n";
    assert_eq!(explain(&project, "defaults.retry", None), expected);
    let set = ["config", "explain", "defaults.retry.attempts", "--set", "defaults.retry.attempts=3"];
    let output = project.vc(&set).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.starts_with("defaults.retry.attempts = 3  (--set)\n  over 1  (vibranium.config.yaml)"), "{text}");
}

#[test]
fn an_unknown_key_is_refused_with_the_closest_one() {
    let project = Project::new(&[CONFIG]);
    let output = project.vc(&["config", "explain", "defaults.timout"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("defaults.timeout"), "{}", said(&output));
}
