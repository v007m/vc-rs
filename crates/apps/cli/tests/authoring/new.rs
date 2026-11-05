//! `vc new api|test|auth|file` append to the file the ID derives from (D139), keep every byte
//! they do not add, and refuse an ID that breaks the conventions with a suggested one (D87).

use vc_mock_server::MockServer;

use crate::{CONFIG, CRUD, Project, USERS, checks_clean, said, schema_line};

const DEACTIVATE: &str = "  - id: deactivate\n    method: POST\n    path: /users/${var.id}/deactivate\n";

#[test]
fn new_api_appends_with_path_parameters_as_vars() {
    let project = Project::new(&[CONFIG, USERS]);
    let args = ["new", "api", "users#deactivate", "--method", "POST", "--path", "/users/{id}/deactivate"];
    let output = project.vc(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let line = USERS.1.lines().count() + 2;
    let added = format!("✓ added users#deactivate to services/users/users.api.yaml:{line}\n");
    assert_eq!(String::from_utf8_lossy(&output.stdout), added, "the line of its `- id:`");
    let text = project.read(USERS.0);
    assert!(text.starts_with(USERS.1), "every existing byte is kept:\n{text}");
    let tail = &text[USERS.1.len()..];
    assert!(tail.starts_with("  # users#deactivate · POST /users/{id}/deactivate"), "a brief comment:\n{tail}");
    assert!(tail.ends_with(DEACTIVATE), "{tail}");
    checks_clean(&project);
}

#[test]
fn new_api_creates_the_file_its_id_derives_from() {
    let project = Project::new(&[CONFIG]);
    project.vc(&["new", "api", "payments.refunds#create", "--method", "POST", "--path", "/refunds"]).assert().code(0);
    let text = project.read("services/payments/refunds.api.yaml");
    assert_eq!(text.lines().next(), Some(schema_line("api").as_str()));
    assert!(text.contains("  - id: create\n    method: POST\n    path: /refunds\n"), "{text}");
    project.vc(&["new", "api", "orders#list", "--path", "/orders"]).assert().code(0);
    assert!(project.read("services/orders/orders.api.yaml").contains("  - id: list\n"));
}

/// D177: the URL's origin becomes a var ending in `Url`, the rest the path and query; GET.
#[test]
fn new_api_from_a_full_url_splits_out_a_base_url_var() {
    let project = Project::new(&[CONFIG]);
    project.vc(&["new", "api", "search#find", "--url", "https://api.example.com/users?q=ann"]).assert().code(0);
    let text = project.read("services/search/search.api.yaml");
    let base = text.lines().find(|line| line.starts_with("baseUrl: ${var.")).unwrap_or_else(|| panic!("{text}"));
    let var = base.trim_start_matches("baseUrl: ${var.").trim_end_matches('}');
    assert!(var.ends_with("Url"), "{var}");
    assert!(text.contains("    path: /users\n") && text.contains("      q: ann\n"), "{text}");
    assert!(!text.contains("method:") || text.contains("method: GET"), "{text}");
    let config = project.read(CONFIG.0);
    assert!(config.contains(var) && config.contains("https://api.example.com"), "{config}");
    checks_clean(&project);
}

#[test]
fn new_api_refuses_an_id_that_breaks_the_convention_and_suggests_one() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["new", "api", "users#Deactivate_User", "--path", "/x"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("deactivateUser"), "{}", said(&output));
    assert_eq!(project.read(USERS.0), USERS.1);
}

#[test]
fn new_api_refuses_an_id_the_file_already_has() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["new", "api", "users#list", "--path", "/x"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("users#list"), "{}", said(&output));
    assert_eq!(project.read(USERS.0), USERS.1);
}

#[tokio::test(flavor = "multi_thread")]
async fn new_test_from_an_api_writes_a_test_that_passes() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS]);
    project.serve_from(&server.base_url());
    project.vc(&["new", "test", "users.crud#listsUsers", "--from", "users#list"]).assert().code(0);
    let text = project.read(CRUD.0);
    assert_eq!(text.lines().next(), Some(schema_line("test").as_str()));
    assert!(text.contains("  - id: listsUsers\n") && text.contains("users#list"), "{text}");
    let output = project.vc(&["run", "users.crud"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
}

/// D176: negative cases are generated from the body schema, so without one there are none.
#[test]
fn new_test_negative_needs_a_body_schema() {
    let project = Project::new(&[CONFIG, USERS]);
    let args = ["new", "test", "users.crud#rejectsBadUser", "--from", "users#create", "--negative"];
    let output = project.vc(&args).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("schema"), "{}", said(&output));
    assert!(!project.exists(CRUD.0), "nothing written");
}

#[test]
fn new_test_refuses_an_unknown_api() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["new", "test", "users.crud#listsUsers", "--from", "users#lst"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("users#list"), "suggests the close match: {}", said(&output));
    assert!(!project.exists(CRUD.0));
}

#[test]
fn new_auth_adds_an_auth_to_the_config() {
    let project = Project::new(&[CONFIG]);
    let output = project.vc(&["new", "auth", "partnerToken", "--type", "bearer"]).write_stdin("\n").output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let config = project.read(CONFIG.0);
    assert!(config.starts_with(CONFIG.1), "the config's bytes are kept:\n{config}");
    assert!(config.contains("id: partnerToken") && config.contains("type: bearer"), "{config}");
}

#[test]
fn new_file_writes_an_empty_file_with_its_header() {
    let project = Project::new(&[CONFIG]);
    project.vc(&["new", "file", "test", "users.crud"]).assert().code(0);
    project.vc(&["new", "file", "api", "users"]).assert().code(0);
    assert_eq!(project.read(CRUD.0).lines().next(), Some(schema_line("test").as_str()));
    assert_eq!(project.read(USERS.0).lines().next(), Some(schema_line("api").as_str()));
    checks_clean(&project);
    let again = project.vc(&["new", "file", "test", "users.crud"]).output().unwrap();
    assert!(!again.status.success(), "an existing file is never overwritten");
}
