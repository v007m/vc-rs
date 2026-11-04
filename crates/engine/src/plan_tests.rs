use vc_testkit::FakeVcs;

use crate::sample::engine;
use crate::{EngineError, PlanRequest, SelectRequest};

fn changed(base: &str) -> PlanRequest {
    PlanRequest {
        select: SelectRequest { changed: Some(base.to_owned()), ..SelectRequest::default() },
        ..PlanRequest::default()
    }
}

#[test]
fn plans_with_the_default_profile() {
    let report = engine(FakeVcs::default()).plan(&PlanRequest::default()).unwrap();
    assert_eq!(report.instance_count(), 3);
    assert!(report.line().ends_with(" · profile dev"), "{}", report.line());
}

#[test]
fn a_named_profile_replaces_the_default_and_later_ones_win() {
    let request = PlanRequest { profiles: vec!["dev".into(), "staging".into()], ..PlanRequest::default() };
    let report = engine(FakeVcs::default()).plan(&request).unwrap();
    assert!(report.line().ends_with(" · profiles dev, staging"), "{}", report.line());
}

#[test]
fn an_unknown_profile_names_the_known_ones() {
    let request = PlanRequest { profiles: vec!["prod".into()], ..PlanRequest::default() };
    let error = engine(FakeVcs::default()).plan(&request).unwrap_err();
    assert_eq!(error.to_string(), "--profile: no profile `prod`");
    assert_eq!(error.help().as_deref(), Some("the profiles are dev, staging"));
}

#[test]
fn repeat_replaces_each_tests_own() {
    let request = PlanRequest { repeat: Some(3), ..PlanRequest::default() };
    assert_eq!(engine(FakeVcs::default()).plan(&request).unwrap().instance_count(), 6);
}

#[test]
fn changed_keeps_the_files_changed_since_the_base() {
    let vcs = FakeVcs::new("main", &["services/orders/flow.test.yaml", "README.md"]);
    let report = engine(vcs).plan(&changed("main")).unwrap();
    assert_eq!(report.instance_count(), 1);
}

#[test]
fn a_ref_the_vcs_refuses_is_a_selection_error_with_help() {
    let error = engine(FakeVcs::default()).plan(&changed("-x")).unwrap_err();
    assert!(matches!(error, EngineError::Selection { .. }), "{error:?}");
    assert!(error.to_string().starts_with("--changed: "), "{error}");
    assert!(error.help().is_some_and(|help| help.contains("--changed origin/main")));
}

#[test]
fn a_profile_from_set_cites_set() {
    let request = PlanRequest { sets: vec!["profile=prod".into()], ..PlanRequest::default() };
    let error = engine(FakeVcs::default()).plan(&request).unwrap_err();
    assert_eq!(error.to_string(), "--set: no profile `prod`");
}

#[test]
fn changed_without_a_ref_compares_against_head() {
    let vcs = FakeVcs::new("HEAD", &["services/users/crud.test.yaml"]);
    assert_eq!(engine(vcs).plan(&changed("")).unwrap().instance_count(), 2);
}

#[test]
fn a_changed_config_or_env_schema_selects_every_test() {
    for shared in ["vibranium.config.yaml", ".env.schema"] {
        let vcs = FakeVcs::new("main", &[shared, "services/orders/flow.test.yaml"]);
        assert_eq!(engine(vcs).plan(&changed("main")).unwrap().instance_count(), 3, "{shared}");
    }
}
