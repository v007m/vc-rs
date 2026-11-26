use std::fs;

use vc_contracts::engine::v1 as wire;
use vc_contracts::results::v1::TestStatus;
use vc_engine::{RunRequest, interrupts};

use super::{compare_runs, test_history};
use crate::Code;
use crate::engine_fixture::{CONFIG, project, serving};

const LISTS: &str = "users.crud#listsUsers";

#[tokio::test]
async fn compares_two_runs_with_the_response_shape_that_changed_between_them() {
    let (dir, api) = project();
    for body in [r#"{"id": 1, "name": "Ada"}"#, r#"{"id": "1", "email": "tok-value-77"}"#] {
        let config = CONFIG.replace("http://127.0.0.1:9", &serving(500, body.as_bytes().to_vec()));
        fs::write(dir.path().join("vibranium.config.yaml"), config).unwrap();
        api.engine().run(&RunRequest::default(), interrupts().1).await.unwrap();
    }
    let request = wire::CompareRunsRequest { a: 1, b: 0, slower_threshold: 0.0 };
    let compared = compare_runs(api, request).await.unwrap();
    assert_eq!((compared.a, compared.b), (1, 2));
    let [still] = compared.still_failing.as_slice() else { panic!("{compared:?}") };
    assert_eq!((still.label.as_str(), still.before(), still.after()), (LISTS, TestStatus::Failed, TestStatus::Failed));
    assert!(compared.newly_failing.is_empty() && compared.fixed.is_empty());
    let [shape] = compared.shapes.as_slice() else { panic!("{compared:?}") };
    assert_eq!((shape.api_id.as_str(), shape.status), ("users#list", 500));
    assert_eq!(
        (shape.added.as_slice(), shape.removed.as_slice()),
        (["/email".to_owned()].as_slice(), ["/name".to_owned()].as_slice())
    );
    let retyped: Vec<_> =
        shape.retyped.iter().map(|path| (path.path.as_str(), path.before.as_str(), path.after.as_str())).collect();
    assert_eq!(retyped, [("/id", "number", "string")]);
    assert!(!format!("{compared:?}").contains("tok-value-77"), "values never leave");
}

#[tokio::test]
async fn refuses_a_negative_threshold_and_a_run_that_is_not_stored() {
    let (_dir, api) = project();
    api.engine().run(&RunRequest::default(), interrupts().1).await.unwrap();
    for share in [-0.1, f64::NAN] {
        let request = wire::CompareRunsRequest { a: 1, b: 1, slower_threshold: share };
        let refused = compare_runs(api.clone(), request).await.unwrap_err();
        assert_eq!(refused.code, Code::InvalidArgument, "{share}");
    }
    let missing = compare_runs(api, wire::CompareRunsRequest { a: 7, b: 0, slower_threshold: 0.2 }).await.unwrap_err();
    assert_eq!(missing.code, Code::FailedPrecondition, "{missing:?}");
}

#[tokio::test]
async fn gives_a_tests_results_newest_first_and_refuses_no_test() {
    let (_dir, api) = project();
    for _ in 0..3 {
        api.engine().run(&RunRequest::default(), interrupts().1).await.unwrap();
    }
    let strip = test_history(api.clone(), wire::TestHistoryRequest { test: LISTS.to_owned(), limit: 2 }).await.unwrap();
    let runs: Vec<_> =
        strip.runs.iter().map(|result| (result.run.as_ref().map(|run| run.id), result.status())).collect();
    assert_eq!(runs, [(Some(3), TestStatus::Failed), (Some(2), TestStatus::Failed)]);
    let [instance] = strip.runs[0].instances.as_slice() else { panic!("{strip:?}") };
    assert_eq!((instance.label.as_str(), instance.exchanges.len()), (LISTS, 0));
    let other = test_history(api.clone(), wire::TestHistoryRequest { test: "users.crud#nope".to_owned(), limit: 0 })
        .await
        .unwrap();
    assert_eq!(
        other.runs.iter().map(|result| (result.status(), result.instances.len())).collect::<Vec<_>>(),
        [(TestStatus::Unspecified, 0); 3]
    );
    let refused = test_history(api, wire::TestHistoryRequest::default()).await.unwrap_err();
    assert_eq!(refused.code, Code::InvalidArgument);
}
