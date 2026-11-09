//! `vc import openapi` writes an api file, its tests, the base-URL var and the snapshot from a
//! served spec, and they compile and run; `vc init --from-openapi` imports into a new project;
//! `vc sync` merges the next version of the spec; a HAR recorded against the mock server
//! imports into its api file and its flow replays green.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, USERS, said};

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

#[tokio::test(flavor = "multi_thread")]
async fn sync_merges_a_changed_spec_and_check_reports_drift() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG]);
    let v1 = format!("{}/openapi/users-v1.yaml", server.base_url());
    project.vc(&["import", "openapi", &v1, "--id", "users"]).assert().code(0);
    project.vc(&["sync", "--check"]).assert().code(0);
    let api_path = project.path().join("services/users/users.api.yaml");
    let api = std::fs::read_to_string(&api_path).unwrap();
    std::fs::write(&api_path, api.replace("users-v1.yaml", "users-v2.yaml")).unwrap();
    project.vc(&["sync", "--check"]).assert().code(4);
    let preview = project.vc(&["sync", "users", "--dry-run"]).output().unwrap();
    assert_eq!(preview.status.code(), Some(0), "{}", said(&preview));
    assert!(said(&preview).contains("+ users#updateUser new operation PATCH /users/${var.id}"), "{}", said(&preview));
    assert!(!std::fs::read_to_string(&api_path).unwrap().contains("updateUser"));
    let synced = project.vc(&["sync", "users"]).output().unwrap();
    assert_eq!(synced.status.code(), Some(0), "{}", said(&synced));
    assert!(said(&synced).contains("- users#deleteUser removed from the spec"), "{}", said(&synced));
    let api = std::fs::read_to_string(&api_path).unwrap();
    assert!(api.contains("operationId: updateUser") && api.contains("deprecated: true"), "{api}");
    project.vc(&["check"]).assert().code(0);
    project.vc(&["sync", "--check"]).assert().code(0);
    let unknown = project.vc(&["sync", "users", "--take", "theirs", "users#getUser"]).output().unwrap();
    assert_eq!(unknown.status.code(), Some(2), "{}", said(&unknown));
}

#[test]
fn a_spec_path_outside_the_project_is_refused() {
    let project = Project::new(&[CONFIG]);
    let output = project.vc(&["import", "openapi", "../elsewhere/openapi.yaml"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("give a URL, or a path inside the project"), "{}", said(&output));
}

/// A session recorded against the mock server, its origin `http://127.0.0.1:7421`.
const RECORDED: &str = include_str!("../fixtures/mock.har");

/// The bearer token the recording sent, which must never be written or printed.
const TOKEN: &str = "e2eRecordedToken7Hq2Lm9Px4";

#[tokio::test(flavor = "multi_thread")]
async fn a_recorded_session_imports_into_its_api_file_and_replays_green() {
    let server = MockServer::start().await.unwrap();
    let config = CONFIG.1.replace("baseUrl: 'http://x'", &format!("baseUrl: '{}'", server.base_url()));
    let project = Project::new(&[(CONFIG.0, &config), USERS]);
    project.write("recorded.har", &RECORDED.replace("http://127.0.0.1:7421", &server.base_url()));
    let flow = "users.recorded#replaysSignup";
    let preview = project.vc(&["import", "har", "recorded.har", "--flow", flow, "--dry-run"]).output().unwrap();
    assert_eq!(preview.status.code(), Some(0), "{}", said(&preview));
    assert!(!project.path().join("services/users/recorded.test.yaml").exists());
    let output = project.vc(&["import", "har", "recorded.har", "--flow", flow]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let host = server.base_url().replace("http://", "");
    for line in [
        "read 9 entries · filtered 1 static, 1 third-party, 1 OPTIONS → 6 requests".to_owned(),
        format!("  {host} → users (existing)  1 matched, 5 new"),
        "flow users.recorded#replaysSignup (6 steps)".to_owned(),
    ] {
        assert!(said(&output).contains(&line), "{line} not in {}", said(&output));
    }
    let written = ["services/users/users.api.yaml", "services/users/recorded.test.yaml", CONFIG.0];
    let texts: Vec<_> =
        written.iter().map(|file| std::fs::read_to_string(project.path().join(file)).unwrap()).collect();
    for text in texts.iter().chain([&said(&preview), &said(&output)]) {
        assert!(!text.contains(TOKEN), "{text}");
    }
    project.vc(&["check"]).assert().code(0);
    let run = project.vc(&["run", flow]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}\n{}", said(&run), texts.join("\n"));
}

/// A collection whose base URL is the mock server's, `http://127.0.0.1:7421`.
const COLLECTION: &str = include_str!("../fixtures/mock.postman.json");

/// The token the collection holds, which must never be written or printed.
const COLLECTION_TOKEN: &str = "e2eCollectionToken8Rk3Wn5Qz";

#[tokio::test(flavor = "multi_thread")]
async fn a_postman_collection_imports_into_its_api_file_and_its_flow_runs_green() {
    let server = MockServer::start().await.unwrap();
    let config = CONFIG.1.replace("baseUrl: 'http://x'", &format!("baseUrl: '{}'", server.base_url()));
    let project = Project::new(&[(CONFIG.0, &config), USERS]);
    project.write("mock.postman.json", &COLLECTION.replace("http://127.0.0.1:7421", &server.base_url()));
    let flow = "users.collection#signsUpAndRenames";
    let preview =
        project.vc(&["import", "postman", "mock.postman.json", "--flow", flow, "--dry-run"]).output().unwrap();
    assert_eq!(preview.status.code(), Some(0), "{}", said(&preview));
    assert!(!project.path().join("services/users/collection.test.yaml").exists());
    let output = project.vc(&["import", "postman", "mock.postman.json", "--flow", flow]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(said(&output).contains("collection Mock · 3 requests"), "{}", said(&output));
    let written = ["services/users/users.api.yaml", "services/users/collection.test.yaml", CONFIG.0];
    let texts: Vec<_> =
        written.iter().map(|file| std::fs::read_to_string(project.path().join(file)).unwrap()).collect();
    for text in texts.iter().chain([&said(&preview), &said(&output)]) {
        assert!(!text.contains(COLLECTION_TOKEN), "{text}");
    }
    project.vc(&["check"]).assert().code(0);
    let run = project.vc(&["run", flow]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}\n{}", said(&run), texts.join("\n"));
}
