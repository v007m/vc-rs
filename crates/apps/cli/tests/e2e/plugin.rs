//! `vc plugin add` from a folder asks before it installs, records what the plugin may reach in
//! `vibranium.lock`, and `vc plugin list` and `remove` read and undo it. A reporter plugin named
//! in `reports.formats` gets the run's events, reads the secret the lock grants, is refused one
//! it does not, and writes its report. An auth plugin named by a `type: plugin` definition signs
//! requests with the headers and cookies it answers.

use std::fs;
use std::process::Command;

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, USERS, said};

/// A reporter plugin in Python's standard library.
const REPORT_PY: &str = include_str!("../fixtures/plugin/report.py");

/// The JSON-RPC framing both Python plugins import.
const RPC: (&str, &str) = ("plugin/rpc.py", include_str!("../fixtures/plugin/rpc.py"));

/// Whether `python3` runs. Starting the interpreter once (site-packages and all) also warms it,
/// so a plugin's own cold start on a loaded runner stays inside the initialize deadline (D217).
/// A non-zero exit, such as the Windows Store alias's, counts as no Python. Under `CI` a missing or
/// broken `python3` fails the test instead of skipping it.
fn has_python() -> bool {
    let ran = Command::new("python3").args(["-c", "pass"]).output();
    if ran.as_ref().is_ok_and(|output| output.status.success()) {
        return true;
    }
    assert!(std::env::var_os("CI").is_none(), "CI needs a working python3 on PATH: {ran:?}");
    eprintln!("skipped: no python3 on PATH");
    false
}

/// A test that lists the users.
const LISTS: (&str, &str) = ("services/users/list.test.yaml", "tests:\n  - id: listsUsers\n    call: users#list\n");

const MANIFEST: &str = concat!(
    "id: report\nversion: 1.0.0\nkind: reporter\n",
    "entry: {command: python3, args: [report.py]}\n",
    "permissions: {secrets: [reportToken]}\n",
);

#[test]
fn a_plugin_is_added_listed_and_removed() {
    let project =
        Project::new(&[CONFIG, ("plugin/vibranium-plugin.yaml", MANIFEST), ("plugin/report.py", "print()\n")]);
    let asked = project.vc(&["plugin", "add", "path:plugin"]).output().unwrap();
    assert_eq!(asked.status.code(), Some(2), "{}", said(&asked));
    assert!(said(&asked).contains("asks to reach: secret reportToken"), "{}", said(&asked));
    assert!(said(&asked).contains("pass --yes"), "{}", said(&asked));
    assert!(!project.path().join("vibranium.lock").exists());
    let added = project.vc(&["plugin", "add", "path:plugin", "--yes"]).output().unwrap();
    assert_eq!(added.status.code(), Some(0), "{}", said(&added));
    let lock = fs::read_to_string(project.path().join("vibranium.lock")).unwrap();
    assert!(lock.contains("      secrets: [reportToken]\n"), "{lock}");
    assert!(project.path().join(".vibranium/plugins/report/report.py").is_file());
    listed_then_removed(&project);
}

/// `vc check` and `vc plugin list` see the added plugin, and `vc plugin remove` undoes it.
fn listed_then_removed(project: &Project) {
    let check = project.vc(&["check"]).output().unwrap();
    assert_eq!(check.status.code(), Some(0), "{}", said(&check));
    let listed = project.vc(&["plugin", "list"]).output().unwrap();
    let text = String::from_utf8_lossy(&listed.stdout).into_owned();
    assert_eq!(text, "report  reporter 1.0.0  path:plugin  ready  may reach: secret reportToken\n");
    let removed = project.vc(&["plugin", "remove", "report"]).output().unwrap();
    assert_eq!(removed.status.code(), Some(0), "{}", said(&removed));
    let config = fs::read_to_string(project.path().join(CONFIG.0)).unwrap();
    assert_eq!(config.trim_end(), CONFIG.1.trim_end());
    assert!(!project.path().join(".vibranium/plugins/report").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reporter_plugin_reads_its_granted_secret_only_and_writes_its_report() {
    if !has_python() {
        return;
    }
    let server = MockServer::start().await.unwrap();
    let schema = (".env.schema", "reportToken=\napiKey=\n");
    let plugin = [("plugin/vibranium-plugin.yaml", MANIFEST), ("plugin/report.py", REPORT_PY)];
    let project = Project::new(&[CONFIG, USERS, LISTS, schema, plugin[0], plugin[1], RPC]);
    let added = project.vc(&["plugin", "add", "path:plugin", "--yes"]).output().unwrap();
    assert_eq!(added.status.code(), Some(0), "{}", said(&added));
    let config = fs::read_to_string(project.path().join(CONFIG.0)).unwrap();
    project.write(CONFIG.0, &format!("{config}reports: {{formats: [ctrf, report]}}\n"));
    let mut run = project.run(&server.base_url(), &["--out", "out"]);
    let run = run.env("reportToken", "t0ken-42").env("apiKey", "k3y").output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let said = said(&run);
    assert!(!said.contains("warning[V5118]"), "the reporter plugin was dropped: {said}");
    assert!(project.path().join("out/ctrf.json").is_file());
    let report = fs::read_to_string(project.path().join("out/plugin/report.txt")).unwrap();
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines[0], "status passed", "{report}");
    assert!(lines[1].strip_prefix("events ").unwrap().parse::<u32>().unwrap() > 0, "{report}");
    assert_eq!(lines[2..], ["reportToken 8 chars", "apiKey refused -32001"], "{report}");
}

/// An auth plugin that answers with a header from its config and a session cookie.
const STAMP_PY: &str = include_str!("../fixtures/plugin/stamp.py");

/// A project whose `partnerStamp` auth is the `stamp` plugin, with a secret in its `config`, and
/// whose two tests call an echo api with it, expecting what the plugin answered the first time.
fn stamped() -> Project {
    let config = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n\
                  auth:\n  - id: partnerStamp\n    type: plugin\n    plugin: stamp\n\
                  \x20   config: {tenant: eu, key: '${secret.STAMP_KEY}'}\n";
    let apis = "baseUrl: ${var.baseUrl}\napis:\n  - id: echo\n    path: /echo\n    auth: partnerStamp\n\
                \x20   expect:\n      body:\n        match:\n          \"headers['x-stamp']\": eu:k3y\n\
                \x20         \"headers['x-calls']\": '1'\n          headers.cookie: session=s-partnerStamp\n";
    let tests =
        "tests:\n  - id: echoesStamped\n    call: partner#echo\n  - id: echoesStampedAgain\n    call: partner#echo\n";
    let manifest = "id: stamp\nversion: 0.1.0\nkind: auth\nentry: {command: python3, args: [stamp.py]}\n";
    Project::new(&[
        ("vibranium.config.yaml", config),
        ("services/partner/partner.api.yaml", apis),
        ("services/partner/stamp.test.yaml", tests),
        (".env.schema", "STAMP_KEY=\n"),
        ("plugin/vibranium-plugin.yaml", manifest),
        ("plugin/stamp.py", STAMP_PY),
        RPC,
    ])
}

/// `type: plugin` sends the rendered `config` to the plugin `plugin` names and applies its
/// headers and cookies; with no `expiresIn` its answer is kept for the run. Before the plugin is
/// added, the call fails with V5114.
#[tokio::test(flavor = "multi_thread")]
async fn an_auth_plugin_signs_every_request_with_what_it_answered_once() {
    if !has_python() {
        return;
    }
    let server = MockServer::start().await.unwrap();
    let project = stamped();
    let refused = project.run(&server.base_url(), &[]).env("STAMP_KEY", "k3y").output().unwrap();
    assert_eq!(refused.status.code(), Some(1), "{}", said(&refused));
    assert!(said(&refused).contains("V5114"), "{}", said(&refused));
    let added = project.vc(&["plugin", "add", "path:plugin", "--yes"]).output().unwrap();
    assert_eq!(added.status.code(), Some(0), "{}", said(&added));
    let run = project.run(&server.base_url(), &["--concurrency", "1"]).env("STAMP_KEY", "k3y").output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    assert_eq!(server.stats().await.unwrap().count("GET /echo"), 2);
}
