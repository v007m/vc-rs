//! `vc sync` fetches a spec behind a bearer token with the api file's `source.headers`, rendered
//! under `--profile`; the file keeps `${secret.…}` as written, and neither the token nor a
//! refusal that names the header shows it (D313). `vc import openapi <url> --header` fetches
//! the same way and writes the headers there; a credential given as a literal is refused
//! (D314, V4009).

use vc_mock_server::{MockHandle, MockServer};

use crate::redaction::files;
use crate::{CONFIG, Project, said};

const TOKEN: &str = "spec-reader";
const SCHEMA: (&str, &str) = (".env.schema", "# the spec's token\nSPEC_TOKEN=\n");
const SIGNED: &str = "  headers:\n    authorization: Bearer ${secret.SPEC_TOKEN}\n";

/// A project whose `users` api file syncs from the mock's private spec with [`SIGNED`].
async fn private_users() -> (MockHandle, Project) {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, SCHEMA]);
    let public = format!("{}/openapi/users.yaml", server.base_url());
    project.vc(&["import", "openapi", &public, "--id", "users"]).assert().code(0);
    let path = project.path().join("services/users/users.api.yaml");
    let api = std::fs::read_to_string(&path).unwrap();
    let line = format!("  openapi: {public}\n");
    assert!(api.contains(&line), "{api}");
    let private = format!("  openapi: {}/openapi/private/users.yaml\n{SIGNED}", server.base_url());
    std::fs::write(&path, api.replace(&line, &private)).unwrap();
    (server, project)
}

/// Every project file, as one text, so a test can show nothing in it holds [`TOKEN`].
fn written(project: &Project) -> String {
    let mut seen = Vec::new();
    files(project.path(), &mut seen);
    seen.into_iter().flat_map(<[String; 2]>::from).collect::<Vec<_>>().join("\n")
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_fetches_a_private_spec_with_its_header_and_keeps_the_template() {
    let (server, project) = private_users().await;
    let synced = project.vc(&["sync", "users", "--profile", "dev"]).env("SPEC_TOKEN", TOKEN).output().unwrap();
    assert_eq!(synced.status.code(), Some(0), "{}", said(&synced));
    let checked = project.vc(&["sync", "--check"]).env("SPEC_TOKEN", TOKEN).output().unwrap();
    assert_eq!(checked.status.code(), Some(0), "{}", said(&checked));
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("GET /openapi/private/users.yaml"), 2, "{stats:?}");
    let files = written(&project);
    assert!(files.contains(SIGNED) && !files.contains(TOKEN), "{files}");
    assert!(!said(&synced).contains(TOKEN) && !said(&checked).contains(TOKEN));
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_refuses_a_header_it_cannot_render_or_a_profile_it_does_not_know_before_any_fetch() {
    let (server, project) = private_users().await;
    let unset = project.vc(&["sync", "users"]).output().unwrap();
    assert_eq!(unset.status.code(), Some(2), "{}", said(&unset));
    assert!(said(&unset).contains("`source.headers.authorization`"), "{}", said(&unset));
    let unknown = project.vc(&["sync", "--profile", "prod"]).env("SPEC_TOKEN", TOKEN).output().unwrap();
    assert_eq!(unknown.status.code(), Some(2), "{}", said(&unknown));
    assert!(said(&unknown).contains("prod") && !said(&unknown).contains(TOKEN), "{}", said(&unknown));
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("GET /openapi/private/users.yaml"), 0, "{stats:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_renders_the_headers_though_another_file_has_errors() {
    let (server, project) = private_users().await;
    let broken = project.path().join("services/users/broken.test.yaml");
    std::fs::write(&broken, "tests:\n  - id: callsNothing\n    call: users#nobody\n").unwrap();
    let checked = project.vc(&["check"]).env("SPEC_TOKEN", TOKEN).output().unwrap();
    assert_ne!(checked.status.code(), Some(0), "the test file has an error: {}", said(&checked));
    let synced = project.vc(&["sync", "users", "--profile", "dev"]).env("SPEC_TOKEN", TOKEN).output().unwrap();
    assert_eq!(synced.status.code(), Some(0), "{}", said(&synced));
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("GET /openapi/private/users.yaml"), 1, "{stats:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_refuses_an_api_file_whose_own_headers_do_not_compile_before_any_fetch() {
    let (server, project) = private_users().await;
    let path = project.path().join("services/users/users.api.yaml");
    let api = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, api.replace("${secret.SPEC_TOKEN}", "${secret.NOT_DECLARED}")).unwrap();
    let synced = project.vc(&["sync", "users", "--profile", "dev"]).env("SPEC_TOKEN", TOKEN).output().unwrap();
    assert_eq!(synced.status.code(), Some(2), "{}", said(&synced));
    assert!(said(&synced).contains("NOT_DECLARED") && !said(&synced).contains(TOKEN), "{}", said(&synced));
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("GET /openapi/private/users.yaml"), 0, "{stats:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn import_fetches_a_private_spec_with_a_secret_header_and_writes_the_template_for_sync() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, SCHEMA]);
    let private = format!("{}/openapi/private/users.yaml", server.base_url());
    let signed = ["--header", "Authorization: Bearer ${secret.SPEC_TOKEN}", "--header", "Accept: application/yaml"];
    let args = [&["import", "openapi", private.as_str(), "--id", "users"][..], &signed[..]].concat();
    let imported = project.vc(&args).env("SPEC_TOKEN", TOKEN).output().unwrap();
    assert_eq!(imported.status.code(), Some(0), "{}", said(&imported));
    let api = std::fs::read_to_string(project.path().join("services/users/users.api.yaml")).unwrap();
    let source = format!(
        "  openapi: {private}\n  headers:\n    Authorization: Bearer ${{secret.SPEC_TOKEN}}\n    Accept: application/yaml\n"
    );
    assert!(api.contains(&source), "{api}");
    let synced = project.vc(&["sync", "users"]).env("SPEC_TOKEN", TOKEN).output().unwrap();
    assert_eq!(synced.status.code(), Some(0), "{}", said(&synced));
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("GET /openapi/private/users.yaml"), 2, "{stats:?}");
    assert!(!written(&project).contains(TOKEN) && !said(&imported).contains(TOKEN));
}

#[tokio::test(flavor = "multi_thread")]
async fn import_refuses_a_literal_bearer_token_with_v4009_and_writes_nothing() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, SCHEMA]);
    let private = format!("{}/openapi/private/users.yaml", server.base_url());
    let bearer = format!("authorization: Bearer {TOKEN}");
    let refused = project.vc(&["import", "openapi", &private, "--header", &bearer]).output().unwrap();
    assert_eq!(refused.status.code(), Some(2), "{}", said(&refused));
    assert!(said(&refused).contains("V4009") && said(&refused).contains("${secret.NAME}"), "{}", said(&refused));
    let malformed = project.vc(&["import", "openapi", &private, "--header", TOKEN]).output().unwrap();
    assert_eq!(malformed.status.code(), Some(2), "{}", said(&malformed));
    assert!(said(&malformed).contains("is not `NAME: VALUE`"), "{}", said(&malformed));
    for output in [&refused, &malformed] {
        assert!(!said(output).contains(TOKEN), "{}", said(output));
    }
    assert!(!project.path().join("services/users").exists() && !written(&project).contains(TOKEN));
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("GET /openapi/private/users.yaml"), 0, "{stats:?}");
}

#[test]
fn check_warns_of_a_literal_token_in_source_headers_without_printing_any_part_of_it() {
    let api = "baseUrl: ${var.baseUrl}\nsource:\n  openapi: https://specs.test/users.yaml\n  headers:\n    \
               Authorization: Bearer sk-live-0042abcd\napis:\n  - id: get\n    path: /users/1\n";
    let project = Project::new(&[CONFIG, ("services/users/users.api.yaml", api)]);
    let text = project.vc(&["check"]).output().unwrap();
    let json = project.vc(&["check", "--json"]).output().unwrap();
    assert!(said(&text).contains("5 |     Authorization: •••\n"), "{}", said(&text));
    for output in [&text, &json] {
        assert_eq!(output.status.code(), Some(0), "{}", said(output));
        assert!(said(output).contains("V4009"), "{}", said(output));
        assert!(!["sk-live", "0042abcd"].iter().any(|part| said(output).contains(part)), "{}", said(output));
    }
}
