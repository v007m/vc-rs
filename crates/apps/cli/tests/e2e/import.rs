//! `vc import openapi` writes an api file, its tests, the base-URL var and the snapshot from a
//! served spec, and they compile and run; `vc init --from-openapi` imports into a new project.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

#[tokio::test(flavor = "multi_thread")]
async fn an_imported_spec_compiles_and_its_tests_run() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG]);
    let spec = format!("{}/openapi/users.yaml", server.base_url());
    let output = project.vc(&["import", "openapi", &spec, "--id", "users", "--tests"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    for file in ["services/users/users.api.yaml", "services/users/openapi.test.yaml", "openapi/users.yaml"] {
        assert!(project.path().join(file).is_file(), "{file}: {}", said(&output));
    }
    let config = std::fs::read_to_string(project.path().join(CONFIG.0)).unwrap();
    assert!(config.contains(&format!("usersUrl: {}", server.base_url())), "{config}");
    project.vc(&["check"]).assert().code(0);
    let run = project.vc(&["run", "users.openapi#listsUsers", "users.openapi#createsUser"]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let again = project.vc(&["import", "openapi", &spec, "--id", "users"]).output().unwrap();
    assert_eq!(again.status.code(), Some(2));
    assert!(said(&again).contains("vc sync users"), "{}", said(&again));
}

#[tokio::test(flavor = "multi_thread")]
async fn init_from_openapi_imports_into_the_new_project() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[]);
    let spec = format!("{}/openapi/users.yaml", server.base_url());
    let output = project.vc(&["init", "--no-sample", "--from-openapi", &spec]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(project.path().join("services/users/users.api.yaml").is_file(), "{}", said(&output));
    project.vc(&["check"]).assert().code(0);
}

#[test]
fn a_spec_path_outside_the_project_is_refused() {
    let project = Project::new(&[CONFIG]);
    let output = project.vc(&["import", "openapi", "../elsewhere/openapi.yaml"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("give a URL, or a path inside the project"), "{}", said(&output));
}
