//! The profiles a run applies: the named ones, in order, else the defaults.

use vc_model::Program;
use vc_planner::{PlanError, active_profiles};

use crate::common::compile_files;

const API: (&str, &str) =
    ("services/users/users.api.yaml", "baseUrl: http://x\napis:\n  - id: list\n    path: /users\n");
const TWO: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\n";

fn names(named: &[&str]) -> Vec<String> {
    named.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn the_defaults_apply_when_none_is_named() {
    let program = compile_files(&[("vibranium.config.yaml", TWO), API]);
    let (ids, active) = active_profiles(&program, &[]).unwrap();
    assert_eq!((ids.len(), active), (1, names(&["dev"])));
}

#[test]
fn named_profiles_replace_the_defaults_in_their_order() {
    let program = compile_files(&[("vibranium.config.yaml", TWO), API]);
    let (ids, active) = active_profiles(&program, &names(&["staging", "dev"])).unwrap();
    assert_eq!(active, names(&["staging", "dev"]));
    assert_eq!(program.str(ids[0].sym()), Some("staging"));
}

#[test]
fn an_unknown_name_lists_the_known_profiles() {
    let program = compile_files(&[("vibranium.config.yaml", TWO), API]);
    let error = active_profiles(&program, &names(&["prod"])).unwrap_err();
    assert_eq!(error, PlanError::UnknownProfile { name: "prod".into(), known: "dev, staging".into() });
    assert_eq!(error.to_string(), "no profile `prod`");
    assert_eq!(error.help().as_deref(), Some("the profiles are dev, staging"));
}

#[test]
fn profiles_with_no_default_need_one_named() {
    let config = "syntaxVersion: 2\nprofiles:\n  - id: dev\n  - id: prod\n";
    let program = compile_files(&[("vibranium.config.yaml", config), API]);
    let error = active_profiles(&program, &[]).unwrap_err();
    assert_eq!(error.to_string(), "no profile is active");
    assert_eq!(error.help().as_deref(), Some("pass `--profile <id>`; the profiles are dev, prod"));
}

/// The compiler refuses a config without profiles (V3101), so the program is built by hand.
#[test]
fn a_program_without_profiles_applies_none() {
    assert_eq!(active_profiles(&Program::default(), &[]).unwrap(), (Vec::new(), Vec::new()));
}
