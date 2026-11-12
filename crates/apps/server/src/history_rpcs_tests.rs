use std::fs;

use vc_contracts::engine::v1 as wire;
use vc_contracts::results::v1::{RunStatus, TestStatus};
use vc_engine::{PlanRequest, RunRequest, interrupts};

use super::{api_coverage, api_trends, decide_approval, get_run, list_approvals, list_runs, trends};
use crate::Code;
use crate::engine_fixture::{CONFIG, project, serving};

#[tokio::test]
async fn an_empty_history_lists_no_run_and_has_no_last_one() {
    let (_dir, api) = project();
    let runs = list_runs(api.clone(), wire::ListRunsRequest::default()).await.unwrap();
    assert_eq!(runs.runs, []);
    let missing = get_run(api, wire::GetRunRequest::default()).await.unwrap_err();
    assert!(missing.message.contains("no run is stored yet"), "{missing:?}");
}

#[tokio::test]
async fn a_stored_run_comes_back_with_its_tests_and_trends() {
    let (_dir, api) = project();
    api.engine().run(&RunRequest::default(), interrupts().1).await.unwrap();
    let runs = list_runs(api.clone(), wire::ListRunsRequest { limit: 0, kind: None, profile: None, branch: None })
        .await
        .unwrap();
    let [listed] = runs.runs.as_slice() else { panic!("{runs:?}") };
    assert_eq!((listed.kind.as_str(), listed.status()), ("run", RunStatus::Failed));
    let found = get_run(api.clone(), wire::GetRunRequest { id: Some(listed.id) }).await.unwrap();
    assert_eq!(found.run.as_ref().map(|run| run.id), Some(listed.id));
    let [test] = found.tests.as_slice() else { panic!("{found:?}") };
    assert_eq!((test.label.as_str(), test.status()), ("users.crud#listsUsers", TestStatus::Failed));
    let found = trends(api, wire::TrendsRequest::default()).await.unwrap();
    let [trend] = found.trends.as_slice() else { panic!("{found:?}") };
    assert_eq!((trend.test_id.as_str(), trend.results, trend.failed), ("users.crud#listsUsers", 1, 1));
}

#[tokio::test]
async fn runs_list_by_kind_with_the_limit_counted_among_them() {
    let (_dir, api) = project();
    for _ in 0..2 {
        api.engine().run(&RunRequest::default(), interrupts().1).await.unwrap();
    }
    let of =
        |kind: &str, limit| wire::ListRunsRequest { limit, kind: Some(kind.to_owned()), profile: None, branch: None };
    let runs = list_runs(api.clone(), of("run", 0)).await.unwrap();
    let [newest, older] = runs.runs.as_slice() else { panic!("{runs:?}") };
    assert_eq!((newest.kind.as_str(), older.kind.as_str()), ("run", "run"));
    let listed = list_runs(api.clone(), of("run", 1)).await.unwrap();
    assert_eq!(listed.runs.iter().map(|run| run.id).collect::<Vec<_>>(), [newest.id]);
    assert_eq!(list_runs(api, of("load", 0)).await.unwrap().runs, []);
}

#[tokio::test]
async fn runs_and_trends_count_only_the_runs_under_the_profile_and_branch_asked_for() {
    let (_dir, api) = project();
    for profile in ["dev", "bare", "bare"] {
        let plan = PlanRequest { profiles: vec![profile.to_owned()], ..PlanRequest::default() };
        api.engine().run(&RunRequest { plan, ..RunRequest::default() }, interrupts().1).await.unwrap();
    }
    let under = |profile: &str, branch: Option<&str>| wire::ListRunsRequest {
        limit: 1,
        profile: Some(profile.to_owned()),
        branch: branch.map(str::to_owned),
        ..wire::ListRunsRequest::default()
    };
    let [dev] = list_runs(api.clone(), under("dev", None)).await.unwrap().runs.try_into().unwrap();
    assert_eq!(dev.profiles, ["dev"], "the limit is counted among the dev runs, though two are newer");
    assert_eq!(list_runs(api.clone(), under("dev", Some("main"))).await.unwrap().runs, [], "no run knows its branch");
    let window = |profile: &str, branch: Option<&str>| wire::TrendsRequest {
        window: 1,
        profile: Some(profile.to_owned()),
        branch: branch.map(str::to_owned),
    };
    let results = |found: wire::TrendsResponse| found.trends.iter().map(|trend| trend.results).collect::<Vec<_>>();
    assert_eq!(results(trends(api.clone(), window("bare", None)).await.unwrap()), [1]);
    assert_eq!(results(trends(api.clone(), window("staging", None)).await.unwrap()), Vec::<u32>::new());
    assert_eq!(results(trends(api, window("bare", Some("main"))).await.unwrap()), Vec::<u32>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn api_trends_sum_each_apis_calls_over_the_runs_under_the_profile_asked_for() {
    let (dir, api) = project();
    let config = CONFIG.replace("http://127.0.0.1:9", &serving(200, b"[]".to_vec()));
    fs::write(dir.path().join("vibranium.config.yaml"), config).unwrap();
    for profile in ["dev", "bare"] {
        let plan = PlanRequest { profiles: vec![profile.to_owned()], ..PlanRequest::default() };
        api.engine().run(&RunRequest { plan, ..RunRequest::default() }, interrupts().1).await.unwrap();
    }
    let under = |profile: Option<&str>| wire::ApiTrendsRequest {
        profile: profile.map(str::to_owned),
        ..wire::ApiTrendsRequest::default()
    };
    let found = api_trends(api.clone(), under(None)).await.unwrap();
    let [listed] = found.trends.as_slice() else { panic!("{found:?}") };
    assert_eq!((listed.api_id.as_str(), listed.method.as_str(), listed.calls), ("users#list", "GET", 2));
    assert_eq!(listed.statuses, [200]);
    assert!(listed.p50_ms.is_some() && listed.p50_ms <= listed.p95_ms, "{listed:?}");
    let [bare] = api_trends(api.clone(), under(Some("bare"))).await.unwrap().trends.try_into().unwrap();
    assert_eq!(bare.calls, 1);
    assert_eq!(api_trends(api.clone(), under(Some("staging"))).await.unwrap().trends, []);
    let on_main = wire::ApiTrendsRequest { branch: Some("main".to_owned()), ..wire::ApiTrendsRequest::default() };
    assert_eq!(api_trends(api, on_main).await.unwrap().trends, [], "no run knows its branch");
}

#[tokio::test]
async fn api_coverage_lists_each_api_with_its_tests_then_the_operations_only_the_snapshot_has() {
    let (dir, api) = project();
    let spec = "openapi: 3.1.0\ninfo: {title: Users, version: '1'}\npaths:\n  /users/{id}:\n    delete: {operationId: removeUser, responses: {'204': {description: gone}}}\n";
    fs::create_dir_all(dir.path().join("openapi")).unwrap();
    fs::write(dir.path().join("openapi/users.yaml"), spec).unwrap();
    let found = api_coverage(api.clone(), wire::ApiCoverageRequest::default()).await.unwrap();
    let rows: Vec<_> =
        found.apis.iter().map(|row| (row.api_id.as_str(), row.method.as_str(), row.tests, row.spec_only)).collect();
    assert_eq!(
        rows,
        [("users#list", "GET", 1, false), ("users#create", "POST", 0, false), ("removeUser", "DELETE", 0, true)]
    );
    assert_eq!(
        (found.apis[2].documented.as_slice(), found.apis[2].file.as_str()),
        (["204".to_owned()].as_slice(), "openapi/users.yaml")
    );
    assert_eq!(found.notes, Vec::<String>::new());
    fs::write(dir.path().join("openapi/users.yaml"), "paths: [unclosed\n").unwrap();
    let broken = api_coverage(api, wire::ApiCoverageRequest::default()).await.unwrap();
    assert_eq!(broken.apis.len(), 2, "{broken:?}");
    let [note] = broken.notes.as_slice() else { panic!("{broken:?}") };
    assert!(note.contains("cannot be read as a spec"), "{note}");
}

#[tokio::test]
async fn a_decision_needs_a_verdict_and_a_known_approval() {
    let (_dir, api) = project();
    let pending = list_approvals(api.clone(), wire::ListApprovalsRequest { pending_only: true }).await.unwrap();
    assert_eq!(pending.approvals, []);
    let none = decide_approval(api.clone(), wire::DecideApprovalRequest { id: 1, verdict: 0 }).await.unwrap_err();
    assert_eq!(none.code, Code::InvalidArgument);
    let unknown = wire::DecideApprovalRequest { id: 7, verdict: wire::Verdict::Approve.into() };
    assert_ne!(decide_approval(api, unknown).await.unwrap_err().code, Code::Internal);
}
