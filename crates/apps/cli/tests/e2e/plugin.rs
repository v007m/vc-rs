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

/// A reporter plugin that writes down each message it reads, then the end of its stdin.
const HEARD_PY: &str = include_str!("../fixtures/plugin/heard.py");

/// Once the run's reports are written, `vc` sends each plugin it started `shutdown`, then closes
/// its stdin, and waits for it to exit before it exits itself: the plugin hears both.
#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_hears_shutdown_then_its_stdin_close_before_vc_exits() {
    if !has_python() {
        return;
    }
    let server = MockServer::start().await.unwrap();
    let manifest = "id: heard\nversion: 1.0.0\nkind: reporter\nentry: {command: python3, args: [heard.py]}\n";
    let plugin = [("plugin/vibranium-plugin.yaml", manifest), ("plugin/heard.py", HEARD_PY)];
    let project = Project::new(&[CONFIG, USERS, LISTS, plugin[0], plugin[1], RPC]);
    let added = project.vc(&["plugin", "add", "path:plugin", "--yes"]).output().unwrap();
    assert_eq!(added.status.code(), Some(0), "{}", said(&added));
    let config = fs::read_to_string(project.path().join(CONFIG.0)).unwrap();
    project.write(CONFIG.0, &format!("{config}reports: {{formats: [heard]}}\n"));
    let run = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let heard = fs::read_to_string(project.path().join(".vibranium/plugin-data/heard/heard.log")).unwrap();
    let last: Vec<&str> = heard.lines().rev().take(3).collect();
    assert_eq!(last, ["EOF", "shutdown", "reporter/finish"], "{heard}");
}

/// `vc plugin add` warns that a plugin's entry program cannot be found, and `vc plugin list`
/// shows it, and a plugin whose installed files changed since the lock (V5118), as not usable.
#[test]
fn the_list_shows_a_plugin_whose_files_changed_or_whose_entry_program_is_missing_as_not_usable() {
    let missing = MANIFEST.replace("command: python3", "command: vibranium-no-such-program");
    let project =
        Project::new(&[CONFIG, ("plugin/vibranium-plugin.yaml", &missing), ("plugin/report.py", "print()\n")]);
    let added = project.vc(&["plugin", "add", "path:plugin", "--yes"]).output().unwrap();
    assert_eq!(added.status.code(), Some(0), "{}", said(&added));
    let entry = "its entry program `vibranium-no-such-program` cannot be found: install it, or fix \
                 `entry.command` in its vibranium-plugin.yaml";
    assert!(said(&added).contains(&format!("  warning: {entry}\n")), "{}", said(&added));
    let listed = project.vc(&["plugin", "list"]).output().unwrap();
    let row = format!("report  reporter 1.0.0  path:plugin  {entry}  may reach: secret reportToken\n");
    assert_eq!(String::from_utf8_lossy(&listed.stdout), row);
    project.write(".vibranium/plugins/report/report.py", "print('changed')\n");
    let listed = project.vc(&["plugin", "list"]).output().unwrap();
    let row = "report  reporter 1.0.0  path:plugin  V5118: its installed files changed since vibranium.lock \
               approved them; `vc plugin add` it again  may reach: secret reportToken\n";
    assert_eq!(String::from_utf8_lossy(&listed.stdout), row);
}

/// A reporter plugin whose installed manifest asks for more than the lock grants is named in
/// one run notice, which says its report is not written.
#[tokio::test(flavor = "multi_thread")]
async fn a_reporter_plugin_asking_for_more_than_the_lock_grants_gets_one_run_notice() {
    let server = MockServer::start().await.unwrap();
    let granted = MANIFEST.replace("{secrets: [reportToken]}", "{}");
    let plugin = [("plugin/vibranium-plugin.yaml", granted.as_str()), ("plugin/report.py", "print()\n")];
    let project = Project::new(&[CONFIG, USERS, LISTS, plugin[0], plugin[1]]);
    let added = project.vc(&["plugin", "add", "path:plugin", "--yes"]).output().unwrap();
    assert_eq!(added.status.code(), Some(0), "{}", said(&added));
    project.write(".vibranium/plugins/report/vibranium-plugin.yaml", MANIFEST);
    let config = fs::read_to_string(project.path().join(CONFIG.0)).unwrap();
    project.write(CONFIG.0, &format!("{config}reports: {{formats: [report]}}\n"));
    let run = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let said = said(&run);
    assert_eq!(said.matches("asks for more").count(), 1, "{said}");
    let notice = "plugin `report`: asks for more: secret reportToken; `vc plugin add` it again; its report is \
                  not written";
    assert!(said.contains(notice), "{said}");
}

/// An `npm` that fails as npm 10 does for a package the registry does not have: its code, the
/// reason, and last the pointer to its log.
#[cfg(unix)]
const NPM_E404: &str = concat!(
    "#!/bin/sh\n",
    "echo 'npm error code E404' >&2\n",
    "echo 'npm error 404 Not Found - GET https://registry.npmjs.org/@acme%2fnothing - Not found' >&2\n",
    "echo 'npm error 404' >&2\n",
    "echo \"npm error 404  '@acme/nothing@*' is not in this registry.\" >&2\n",
    "echo 'npm error A complete log of this run can be found in: /home/a/.npm/_logs/debug-0.log' >&2\n",
    "exit 1\n",
);

/// `vc plugin add` says why npm could not fetch a package, not where npm logged it, and a
/// `path:` source it cannot fetch gets help about the folder.
#[cfg(unix)]
#[test]
fn a_source_that_cannot_be_fetched_says_npm_s_reason_or_points_at_the_folder() {
    use std::os::unix::fs::PermissionsExt as _;
    let project = Project::new(&[CONFIG, ("bin/npm", NPM_E404)]);
    let npm = project.path().join("bin/npm");
    fs::set_permissions(&npm, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(project.path().join("bin")).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let fetched = project.vc(&["plugin", "add", "npm:@acme/nothing", "--yes"]).env("PATH", path).output().unwrap();
    assert_eq!(fetched.status.code(), Some(2), "{}", said(&fetched));
    let reason = "error: cannot fetch `npm:@acme/nothing`: `npm` failed: E404: 404 Not Found - GET \
                  https://registry.npmjs.org/@acme%2fnothing - Not found\n  = help: check the package name and \
                  range, and that `npm` is on the path and can reach its registry\n";
    assert!(said(&fetched).contains(reason), "{}", said(&fetched));
    let folder = project.vc(&["plugin", "add", "path:nope", "--yes"]).output().unwrap();
    assert_eq!(folder.status.code(), Some(2), "{}", said(&folder));
    let help = "  = help: check the folder: a `path:` source is a folder, from the project root, holding \
                vibranium-plugin.yaml\n";
    assert!(said(&folder).contains(help), "{}", said(&folder));
}
