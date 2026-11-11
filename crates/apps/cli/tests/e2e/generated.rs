//! The tests `vc import openapi --tests` writes for the mock users spec create the user each
//! call needs first and remove it afterwards, and check every body against its schema in the
//! snapshot: `vc check` finds nothing to warn about and every test passes.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

#[tokio::test(flavor = "multi_thread")]
async fn generated_tests_check_clean_and_pass_against_the_spec_s_server() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG]);
    let spec = format!("{}/openapi/users.yaml", server.base_url());
    let output = project.vc(&["import", "openapi", &spec, "--id", "users", "--tests"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let tests = std::fs::read_to_string(project.path().join("services/users/openapi.test.yaml")).unwrap();
    for written in
        ["- call: users#createUser\n        keep: true", "$file: ../../openapi/users.yaml#/components/schemas/User"]
    {
        assert!(tests.contains(written), "{written}: {tests}");
    }
    let check = project.vc(&["check"]).output().unwrap();
    assert_eq!(check.status.code(), Some(0), "{}", said(&check));
    assert!(said(&check).contains("0 errors · 0 warnings"), "{}", said(&check));
    let run = project.vc(&["run"]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
}
