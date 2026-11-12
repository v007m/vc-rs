use vc_testkit::FakeVcs;

use crate::sample::engine;
use crate::{EngineError, PlanRequest, SelectRequest};

fn changed(base: &str) -> PlanRequest {
    PlanRequest {
        select: SelectRequest { changed: Some(base.to_owned()), ..SelectRequest::default() },
        ..PlanRequest::default()
    }
}

#[tokio::test]
async fn plans_with_the_default_profile() {
    let report = engine(FakeVcs::default()).plan(&PlanRequest::default()).await.unwrap();
    assert_eq!(report.instance_count(), 3);
    assert!(report.line().ends_with(" · profile dev"), "{}", report.line());
}

#[tokio::test]
async fn a_named_profile_replaces_the_default_and_later_ones_win() {
    let request = PlanRequest { profiles: vec!["dev".into(), "staging".into()], ..PlanRequest::default() };
    let report = engine(FakeVcs::default()).plan(&request).await.unwrap();
    assert!(report.line().ends_with(" · profiles dev, staging"), "{}", report.line());
}

#[tokio::test]
async fn an_unknown_profile_names_the_known_ones() {
    let request = PlanRequest { profiles: vec!["prod".into()], ..PlanRequest::default() };
    let error = engine(FakeVcs::default()).plan(&request).await.unwrap_err();
    assert_eq!(error.to_string(), "--profile: no profile `prod`");
    assert_eq!(error.help().as_deref(), Some("the profiles are dev, staging"));
}

#[tokio::test]
async fn repeat_replaces_each_tests_own() {
    let request = PlanRequest { repeat: Some(3), ..PlanRequest::default() };
    assert_eq!(engine(FakeVcs::default()).plan(&request).await.unwrap().instance_count(), 6);
}

#[tokio::test]
async fn changed_keeps_the_files_changed_since_the_base() {
    let vcs = FakeVcs::new("main", &["services/orders/flow.test.yaml", "README.md"]);
    let report = engine(vcs).plan(&changed("main")).await.unwrap();
    assert_eq!(report.instance_count(), 1);
}

#[tokio::test]
async fn a_ref_the_vcs_refuses_is_a_selection_error_with_help() {
    let error = engine(FakeVcs::default()).plan(&changed("-x")).await.unwrap_err();
    assert!(matches!(error, EngineError::Selection { .. }), "{error:?}");
    assert!(error.to_string().starts_with("--changed: "), "{error}");
    assert!(error.help().is_some_and(|help| help.contains("--changed origin/main")));
}

#[tokio::test]
async fn a_profile_from_set_cites_set() {
    let request = PlanRequest { sets: vec!["profile=prod".into()], ..PlanRequest::default() };
    let error = engine(FakeVcs::default()).plan(&request).await.unwrap_err();
    assert_eq!(error.to_string(), "--set: no profile `prod`");
}

#[tokio::test]
async fn changed_without_a_ref_compares_against_head() {
    let vcs = FakeVcs::new("HEAD", &["services/users/crud.test.yaml"]);
    assert_eq!(engine(vcs).plan(&changed("")).await.unwrap().instance_count(), 2);
}

#[tokio::test]
async fn a_changed_config_or_env_schema_selects_every_test() {
    for shared in ["vibranium.config.yaml", ".env.schema"] {
        let vcs = FakeVcs::new("main", &[shared, "services/orders/flow.test.yaml"]);
        assert_eq!(engine(vcs).plan(&changed("main")).await.unwrap().instance_count(), 3, "{shared}");
    }
}

fn saved(paths: &[&str]) -> PlanRequest {
    let saved = paths.iter().map(|path| vc_model::SourcePath::new(path).unwrap()).collect();
    PlanRequest { select: SelectRequest { saved: Some(saved), ..SelectRequest::default() }, ..PlanRequest::default() }
}

#[tokio::test]
async fn saved_keeps_the_saved_files_and_the_tests_calling_a_saved_api() {
    let engine = engine(FakeVcs::default());
    assert_eq!(engine.plan(&saved(&["services/orders/flow.test.yaml"])).await.unwrap().instance_count(), 1);
    assert_eq!(engine.plan(&saved(&["services/users/users.api.yaml"])).await.unwrap().instance_count(), 3);
}

#[tokio::test]
async fn saved_stands_in_for_changed() {
    let mut request = saved(&["services/orders/flow.test.yaml"]);
    request.select.changed = Some("-x".to_owned());
    assert_eq!(engine(FakeVcs::default()).plan(&request).await.unwrap().instance_count(), 1);
}

#[tokio::test]
async fn a_saved_config_env_or_env_schema_selects_every_test() {
    for shared in ["vibranium.config.yaml", ".env.schema", ".env"] {
        let request = saved(&[shared, "services/orders/flow.test.yaml"]);
        assert_eq!(engine(FakeVcs::default()).plan(&request).await.unwrap().instance_count(), 3, "{shared}");
    }
}

#[tokio::test]
async fn a_changed_env_is_not_shared() {
    let vcs = FakeVcs::new("main", &[".env", "services/orders/flow.test.yaml"]);
    assert_eq!(engine(vcs).plan(&changed("main")).await.unwrap().instance_count(), 1);
}

fn changed_since(days: u64) -> PlanRequest {
    let window = Some(std::time::Duration::from_hours(days * 24));
    PlanRequest {
        select: SelectRequest { changed_since: window, ..SelectRequest::default() },
        ..PlanRequest::default()
    }
}

#[tokio::test]
async fn changed_since_keeps_the_files_changed_lately_and_a_shared_one_selects_every_test() {
    let vcs = FakeVcs::default().with_recent(&["services/orders/flow.test.yaml"]);
    assert_eq!(engine(vcs).plan(&changed_since(7)).await.unwrap().instance_count(), 1);
    let vcs = FakeVcs::default().with_recent(&["vibranium.config.yaml"]);
    assert_eq!(engine(vcs).plan(&changed_since(7)).await.unwrap().instance_count(), 3);
}

#[tokio::test]
async fn changed_wins_over_changed_since() {
    let vcs = FakeVcs::new("main", &[]).with_recent(&["services/orders/flow.test.yaml"]);
    let mut request = changed_since(7);
    request.select.changed = Some("main".to_owned());
    assert_eq!(engine(vcs).plan(&request).await.unwrap().instance_count(), 0);
}
