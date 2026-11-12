//! `vc import openapi` writes an api file, its tests, the base-URL var and the snapshot from a
//! served spec, and they compile and run; `vc init --from-openapi` imports into a new project;
//! `vc sync` merges the next version of the spec; a HAR recorded against the mock server, and a
//! Postman or Bruno collection calling it (a Bruno one as its export or its folder), import into
//! its api file and their flows run green; a folder without `bruno.json` is refused; a recording
//! whose every host is third-party names them, and `--include-host` previews their new files;
//! an `--include` or `--only` that keeps no operation is refused and writes nothing;
//! no secret in a recording is said or written; import and sync refuse a snapshot path that is
//! the spec, a link or another file's name in another case, and sync replaces a snapshot hard
//! linked to the spec rather than writing into it.

use vc_mock_server::MockServer;

use crate::redaction::files;
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

#[tokio::test(flavor = "multi_thread")]
async fn a_filter_that_keeps_no_operation_is_refused_and_writes_nothing() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG]);
    let spec = format!("{}/openapi/users.yaml", server.base_url());
    let output = project.vc(&["import", "openapi", &spec, "--id", "users", "--only", "nope"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("no operation in"), "{}", said(&output));
    assert!(said(&output).contains("matches the filter `--only nope`"), "{}", said(&output));
    assert!(!project.path().join("services").exists() && !project.path().join("openapi").exists());
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

/// A collection from an api client, whose base URL is the mock server's, `http://127.0.0.1:7421`:
/// the files it is written as and the one (or the folder) imported.
struct Collection {
    kind: &'static str,
    source: &'static str,
    files: &'static [(&'static str, &'static str)],
    /// The token it holds, which must never be written or printed.
    token: &'static str,
}

const POSTMAN: Collection = Collection {
    kind: "postman",
    source: "mock.postman.json",
    files: &[("mock.postman.json", include_str!("../fixtures/mock.postman.json"))],
    token: "e2eCollectionToken8Rk3Wn5Qz",
};

const BRUNO: Collection = Collection {
    kind: "bruno",
    source: "mock.bruno.json",
    files: &[("mock.bruno.json", include_str!("../fixtures/mock.bruno.json"))],
    token: "e2eBrunoToken5Vd8Kq2Jx6Wm",
};

/// The same Bruno collection as its folder, as Bruno keeps it.
const BRUNO_FOLDER: Collection = Collection {
    kind: "bruno",
    source: "bruno/mock",
    files: &[
        ("bruno/mock/bruno.json", include_str!("../fixtures/mock.bruno/bruno.json")),
        ("bruno/mock/collection.bru", include_str!("../fixtures/mock.bruno/collection.bru")),
        ("bruno/mock/environments/Local.bru", include_str!("../fixtures/mock.bruno/environments/Local.bru")),
        ("bruno/mock/get-user.bru", include_str!("../fixtures/mock.bruno/get-user.bru")),
        ("bruno/mock/rename-user.bru", include_str!("../fixtures/mock.bruno/rename-user.bru")),
        ("bruno/mock/sign-up.bru", include_str!("../fixtures/mock.bruno/sign-up.bru")),
    ],
    token: "e2eBrunoToken5Vd8Kq2Jx6Wm",
};

/// Imports `collection` with `--flow`, previewed first, and runs the flow against the mock server.
async fn imports_and_runs_green(collection: &Collection) {
    let server = MockServer::start().await.unwrap();
    let config = CONFIG.1.replace("baseUrl: 'http://x'", &format!("baseUrl: '{}'", server.base_url()));
    let project = Project::new(&[(CONFIG.0, &config), USERS]);
    for (file, text) in collection.files {
        project.write(file, &text.replace("http://127.0.0.1:7421", &server.base_url()));
    }
    let flow = "users.collection#signsUpAndRenames";
    let args = ["import", collection.kind, collection.source, "--flow", flow];
    let preview = project.vc(&args).arg("--dry-run").output().unwrap();
    assert_eq!(preview.status.code(), Some(0), "{}", said(&preview));
    assert!(!project.path().join("services/users/collection.test.yaml").exists());
    let output = project.vc(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(said(&output).contains("collection Mock · 3 requests"), "{}", said(&output));
    let written = ["services/users/users.api.yaml", "services/users/collection.test.yaml", CONFIG.0];
    let texts: Vec<_> =
        written.iter().map(|file| std::fs::read_to_string(project.path().join(file)).unwrap()).collect();
    for text in texts.iter().chain([&said(&preview), &said(&output)]) {
        assert!(!text.contains(collection.token), "{text}");
    }
    project.vc(&["check"]).assert().code(0);
    let run = project.vc(&["run", flow]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}\n{}", said(&run), texts.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_postman_collection_imports_into_its_api_file_and_its_flow_runs_green() {
    imports_and_runs_green(&POSTMAN).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bruno_export_imports_into_its_api_file_and_its_flow_runs_green() {
    imports_and_runs_green(&BRUNO).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bruno_collection_folder_imports_into_its_api_file_and_its_flow_runs_green() {
    imports_and_runs_green(&BRUNO_FOLDER).await;
}

#[test]
fn a_folder_without_bruno_json_is_refused() {
    let project = Project::new(&[CONFIG, ("bruno/mock/sign-up.bru", "meta {\n  name: Sign up\n}\n")]);
    let output = project.vc(&["import", "bruno", "bruno/mock"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    let line = "it is not a Bruno collection: the folder has no bruno.json";
    assert!(said(&output).contains(line), "{line} not in {}", said(&output));
}

#[test]
fn a_recording_to_a_host_no_api_file_is_on_names_it_and_include_host_previews_its_new_file() {
    let project = Project::new(&[CONFIG, USERS]);
    project.write("recorded.har", RECORDED);
    let refused = project.vc(&["import", "har", "recorded.har"]).output().unwrap();
    assert_eq!(refused.status.code(), Some(2), "{}", said(&refused));
    for line in ["`--include-host www.google-analytics.com` or `--include-host 127.0.0.1`", "`--include-host <host>`"] {
        assert!(said(&refused).contains(line), "{line} not in {}", said(&refused));
    }
    assert!(!said(&refused).contains("--only"), "{}", said(&refused));
    let kept = ["import", "har", "recorded.har", "--include-host", "127.0.0.1", "--dry-run"];
    let preview = project.vc(&kept).output().unwrap();
    assert_eq!(preview.status.code(), Some(0), "{}", said(&preview));
    for line in ["  127.0.0.1:7421 → recorded (new)  6 new", "+++ b/services/recorded/recorded.api.yaml"] {
        assert!(said(&preview).contains(line), "{line} not in {}", said(&preview));
    }
    assert!(!project.path().join("services/recorded").exists());
    assert!(!said(&preview).contains(TOKEN), "{}", said(&preview));
}

/// A shop session recorded against two origins, `https://api.shop.test` and `https://pay.shop.test`.
const TWO_ORIGINS: &str = include_str!("../../../../adapters/importers/fixtures/shop.har");

#[test]
fn an_id_for_a_recording_of_two_origins_is_refused_naming_them() {
    let project = Project::new(&[CONFIG]);
    project.write("shop.har", TWO_ORIGINS);
    let hosts = ["--include-host", "api.shop.test", "--include-host", "pay.shop.test"];
    let output = project.vc(&[&["import", "har", "shop.har", "--id", "shop"][..], &hosts].concat()).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    let origins = "https://api.shop.test, https://pay.shop.test";
    let reason = format!("`--id shop` names one api file, but the recording calls 2 origins: {origins}");
    for line in [reason.as_str(), "import without `--id`: each origin goes to the api file"] {
        assert!(said(&output).contains(line), "{line} not in {}", said(&output));
    }
    assert!(!said(&output).contains("pk_9Rt2Lm8Qx4Vz7Nb3Kc6Wd1Hy"), "{}", said(&output));
    assert!(!project.path().join("services").exists());
}

/// What [`TWO_ORIGINS`] holds that no output or written file may, as the engine's own test lists
/// them: a bearer token, a session cookie, an api key, a nonce, a card token, a password and a
/// token a response gave back.
const SHOP_SECRETS: [&str; 7] = [
    "eyJhbGciOiJIUzI1NiJ9",
    "s3ss10nC00k1e",
    "pk_9Rt2Lm8Qx4Vz7Nb3Kc6Wd1Hy",
    "Xk9pQ2mZ7vR4tL8wB3nY6cH1",
    "tok_visa",
    "hunter2",
    "Zq8Wv3Lp5Rt7Ny2Kb4Hd6Fj9",
];

#[test]
fn a_recording_of_two_origins_imports_and_no_secret_in_it_is_said_or_written() {
    let project = Project::new(&[CONFIG]);
    project.write("shop.har", TWO_ORIGINS);
    let hosts = ["--include-host", "api.shop.test", "--include-host", "pay.shop.test"];
    let output = project.vc(&[&["import", "har", "shop.har"][..], &hosts].concat()).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let mut written = Vec::new();
    files(project.path(), &mut written);
    written.retain(|(path, _)| !path.ends_with("shop.har"));
    assert!(written.iter().any(|(path, _)| path.ends_with(".api.yaml")), "{written:?}");
    let leaked: Vec<_> = SHOP_SECRETS
        .iter()
        .filter(|secret| said(&output).contains(*secret) || written.iter().any(|(_, text)| text.contains(*secret)))
        .collect();
    assert!(leaked.is_empty(), "{leaked:?} in {} or {written:?}", said(&output));
}

/// A spec with one operation, kept in the project.
const SPEC: (&str, &str) = (
    "specs/users.yaml",
    "openapi: 3.1.0\ninfo: {title: Users, version: '1'}\npaths:\n  /users:\n    get:\n      operationId: listUsers\n\
     \x20     responses: {'200': {description: ok}}\n",
);

#[cfg(unix)]
#[test]
fn a_snapshot_path_that_links_to_the_spec_is_refused_and_the_spec_is_left_as_it_was() {
    let project = Project::new(&[CONFIG, SPEC]);
    std::fs::create_dir_all(project.path().join("openapi")).unwrap();
    std::os::unix::fs::symlink(project.path().join(SPEC.0), project.path().join("openapi/users.yaml")).unwrap();
    for source in [SPEC.0, "openapi/users.yaml"] {
        let output = project.vc(&["import", "openapi", source, "--id", "users"]).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{source}: {}", said(&output));
        assert!(
            said(&output)
                .contains("openapi/users.yaml, where `vc import` keeps the snapshot, leads to specs/users.yaml")
        );
    }
    assert_eq!(std::fs::read_to_string(project.path().join(SPEC.0)).unwrap(), SPEC.1);
}

#[cfg(target_os = "macos")]
#[test]
fn a_snapshot_path_that_differs_only_in_case_from_a_file_is_refused() {
    let snapshot = ("openapi/users.yaml", "# kept by an earlier import\n");
    let project = Project::new(&[CONFIG, SPEC, snapshot]);
    let output = project.vc(&["import", "openapi", SPEC.0, "--id", "Users"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(
        said(&output).contains("openapi/Users.yaml, where `vc import` keeps the snapshot, leads to openapi/users.yaml")
    );
    assert_eq!(std::fs::read_to_string(project.path().join(snapshot.0)).unwrap(), snapshot.1);
}

/// The spec of [`SPEC`] with a server, so it imports into a file that compiles.
const SERVED: (&str, &str) = (
    "specs/users.yaml",
    "openapi: 3.1.0\ninfo: {title: Users, version: '1'}\nservers: [{url: 'http://127.0.0.1:9'}]\npaths:\n  /users:\n\
     \x20   get:\n      operationId: listUsers\n      responses: {'200': {description: ok}}\n",
);

/// A project with [`SERVED`] imported as `users`, its snapshot moved to `specs/old-snapshot.yaml`.
fn imported_without_its_snapshot() -> Project {
    let project = Project::new(&[CONFIG, SERVED]);
    let output = project.vc(&["import", "openapi", SERVED.0, "--id", "users"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let at = |path: &str| project.path().join(path);
    std::fs::rename(at("openapi/users.yaml"), at("specs/old-snapshot.yaml")).unwrap();
    project
}

#[cfg(unix)]
#[test]
fn sync_refuses_a_snapshot_path_that_links_to_the_spec_and_leaves_the_spec_as_it_was() {
    let project = imported_without_its_snapshot();
    std::os::unix::fs::symlink(project.path().join(SERVED.0), project.path().join("openapi/users.yaml")).unwrap();
    let output = project.vc(&["sync", "users"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    let leads = "openapi/users.yaml, where `vc import` keeps the snapshot, leads to specs/users.yaml";
    assert!(said(&output).contains(leads), "{}", said(&output));
    assert_eq!(std::fs::read_to_string(project.path().join(SERVED.0)).unwrap(), SERVED.1);
}

#[cfg(target_os = "macos")]
#[test]
fn sync_refuses_a_spec_kept_at_its_snapshot_path_in_another_case() {
    let project = imported_without_its_snapshot();
    let at = |path: &str| project.path().join(path);
    std::fs::rename(at(SERVED.0), at("openapi/Users.yaml")).unwrap();
    let api = std::fs::read_to_string(at("services/users/users.api.yaml")).unwrap();
    assert!(api.contains("openapi: specs/users.yaml"), "{api}");
    let moved = api.replace("openapi: specs/users.yaml", "openapi: openapi/Users.yaml");
    std::fs::write(at("services/users/users.api.yaml"), moved).unwrap();
    let output = project.vc(&["sync", "users"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    let leads = "openapi/users.yaml, where `vc import` keeps the snapshot, leads to openapi/Users.yaml";
    assert!(said(&output).contains(leads), "{}", said(&output));
    assert_eq!(std::fs::read_to_string(at("openapi/Users.yaml")).unwrap(), SERVED.1);
}

#[cfg(unix)]
#[test]
fn sync_replaces_a_snapshot_hard_linked_to_the_spec_and_leaves_the_spec_as_it_was() {
    let project = imported_without_its_snapshot();
    let at = |path: &str| project.path().join(path);
    std::fs::hard_link(at(SERVED.0), at("openapi/users.yaml")).unwrap();
    let output = project.vc(&["sync", "users"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert_eq!(std::fs::read_to_string(at(SERVED.0)).unwrap(), SERVED.1);
    let snapshot = std::fs::read_to_string(at("openapi/users.yaml")).unwrap();
    assert!(snapshot.starts_with("# Snapshot of specs/users.yaml"), "{snapshot}");
}
