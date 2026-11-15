//! The run history: each run is stored with its report snapshot and gets a report folder of
//! its own; `vc history` lists and shows the runs, `vc report` writes a stored run's files
//! again, and `vc run --failed` reruns what failed under the same profiles; a test marked `skip`
//! stays skipped unless `--include-skipped` (D328).

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use vc_mock_server::MockServer;

use crate::{CONFIG, Project, USERS, events, of_kind, said};

const MIXED: (&str, &str) = (
    "services/users/mixed.test.yaml",
    "tests:\n  - id: listsUsers\n    call: users#list\n\
     \x20 - id: wantsAnError\n    call: users#list\n    expect: {status: 500}\n\
     \x20 - id: isParked\n    call: users#list\n    skip: {reason: later}\n",
);

/// A parked test and a live one, each expecting a status the mock never answers.
const PARKED: (&str, &str) = (
    "services/users/parked.test.yaml",
    "tests:\n  - id: isParked\n    call: users#list\n    expect: {status: 500}\n    skip: {reason: later}\n\
     \x20 - id: wantsAnError\n    call: users#list\n    expect: {status: 500}\n",
);

fn json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn stores_each_run_and_reruns_what_failed() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, MIXED]);
    project.run(&server.base_url(), &[]).assert().code(1);
    assert!(project.path().join(".vibranium/reports/1/ctrf.json").is_file(), "run 1 has a folder of its own");

    let rerun = project.run(&server.base_url(), &["--failed", "--reporter", "json"]).output().unwrap();
    assert_eq!(rerun.status.code(), Some(1), "{}", said(&rerun));
    let finished = events(&rerun.stdout);
    let tests: Vec<&Value> = of_kind(&finished, "testFinished");
    assert_eq!(tests.len(), 1, "only the failed test runs again: {finished:#?}");

    let runs = json(&project.vc(&["history", "--json"]).output().unwrap());
    assert_eq!(runs.as_array().map(Vec::len), Some(2), "{runs:#}");
    assert_eq!(
        (&runs[0]["id"], &runs[0]["selector"], &runs[0]["profiles"][0]),
        (&2.into(), &"--failed".into(), &"dev".into())
    );
    assert_eq!((&runs[1]["status"], &runs[1]["totals"]["passed"]), (&"failed".into(), &1.into()));

    let shown = project.vc(&["history", "show", "1"]).output().unwrap();
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(
        text.contains("failed   users.mixed#wantsAnError") && text.contains("skipped  users.mixed#isParked"),
        "{text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_a_stored_runs_report_again_and_prunes() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, MIXED]);
    project.run(&server.base_url(), &[]).assert().code(1);
    project.run(&server.base_url(), &["users.mixed#listsUsers"]).assert().code(0);

    let written = project.vc(&["report", "1", "--single-file"]).output().unwrap();
    assert_eq!(written.status.code(), Some(0), "{}", said(&written));
    let page = project.path().join(".vibranium/reports/1/report.html");
    let printed = PathBuf::from(String::from_utf8_lossy(&written.stdout).trim());
    assert_eq!(printed.canonicalize().unwrap(), page.canonicalize().unwrap());
    assert!(fs::read_to_string(&page).unwrap().contains("<title>Vibranium run 1</title>"));

    let folder = project.vc(&["report", "--format", "html,junit", "--out", "again"]).output().unwrap();
    assert_eq!(folder.status.code(), Some(0), "{}", said(&folder));
    for file in ["again/index.html", "again/data/summary.js", "again/junit.xml"] {
        assert!(project.path().join(file).is_file(), "{file}: {}", said(&folder));
    }

    project.vc(&["history", "prune", "--keep", "1"]).assert().code(0).stdout("pruned 1 run\n");
    let gone = project.vc(&["report", "1"]).output().unwrap();
    assert_eq!(gone.status.code(), Some(2));
    assert!(said(&gone).contains("no run 1 is stored"), "{}", said(&gone));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pruned_runs_report_folder_goes_with_it() {
    let server = MockServer::start().await.unwrap();
    let config = format!("{}history: {{keep: 2}}\n", CONFIG.1);
    let project = Project::new(&[(CONFIG.0, config.as_str()), USERS, MIXED]);
    let reports = project.path().join(".vibranium/reports");
    for _ in 0..3 {
        project.run(&server.base_url(), &["users.mixed#listsUsers"]).assert().code(0);
    }
    fs::write(reports.join("notes.txt"), "mine").unwrap();
    fs::create_dir(reports.join("0")).unwrap();
    fs::write(reports.join("0/notes.txt"), "mine").unwrap();
    let folders = || ["1", "2", "3"].map(|run| reports.join(run).is_dir());
    assert_eq!(folders(), [false, true, true], "`history.keep: 2` keeps the folders of runs 2 and 3");
    assert!(reports.join("3/.vc-run").is_file(), "vc marks each run's folder as its own");
    project.vc(&["history", "prune", "--keep", "1"]).assert().code(0).stdout("pruned 1 run\n");
    assert_eq!(folders(), [false, false, true], "`vc history prune --keep 1` drops run 2's folder");
    assert!(reports.join("notes.txt").is_file(), "anything not a run's folder stays");
    assert!(reports.join("0/notes.txt").is_file(), "a folder named by a number that vc did not mark stays");
}

/// With `history.keep: 0` no run is kept, yet each run keeps the report folder it just printed;
/// a folder of the user's own named by a number stays, and pruning an empty history removes no
/// folder.
#[tokio::test(flavor = "multi_thread")]
async fn keep_zero_keeps_the_folder_the_run_printed_and_an_empty_history_prunes_none() {
    let server = MockServer::start().await.unwrap();
    let config = format!("{}history: {{keep: 0}}\n", CONFIG.1);
    let project = Project::new(&[(CONFIG.0, config.as_str()), USERS, MIXED]);
    let reports = project.path().join(".vibranium/reports");
    fs::create_dir_all(reports.join("0")).unwrap();
    fs::write(reports.join("0/notes.txt"), "mine").unwrap();
    for _ in 0..2 {
        let run = project.run(&server.base_url(), &["users.mixed#listsUsers"]).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{}", said(&run));
        let printed = format!(".vibranium{0}reports{0}1{0}", std::path::MAIN_SEPARATOR);
        assert!(said(&run).contains(&printed), "{}", said(&run));
        assert!(!said(&run).contains(r"\\?\"), "the path is printed as the user writes it: {}", said(&run));
        assert!(reports.join("1/ctrf.json").is_file(), "the folder the run printed is still there");
    }
    project.vc(&["history", "prune", "--keep", "0"]).assert().code(0).stdout("pruned 0 runs\n");
    assert!(reports.join("1/ctrf.json").is_file(), "an empty history removes no folder");
    assert!(reports.join("0/notes.txt").is_file(), "a folder vc did not mark stays");
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_needs_a_failure_to_rerun() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, MIXED]);
    let first = project.run(&server.base_url(), &["--failed"]).output().unwrap();
    assert_eq!(first.status.code(), Some(2));
    assert!(said(&first).contains("no run is stored yet"), "{}", said(&first));
    project.run(&server.base_url(), &["users.mixed#listsUsers"]).assert().code(0);
    let clean = project.run(&server.base_url(), &["--failed"]).output().unwrap();
    assert_eq!(clean.status.code(), Some(2));
    assert!(said(&clean).contains("no failed or blocked test"), "{}", said(&clean));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_is_not_the_last_run() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, MIXED]);
    project.run(&server.base_url(), &[]).assert().code(1);
    let base = format!("vars.baseUrl={}", server.base_url());
    project.vc(&["call", "users#list", "--set", &base]).assert().code(0);

    let shown = project.vc(&["history", "show"]).output().unwrap();
    assert_eq!(shown.status.code(), Some(0), "{}", said(&shown));
    assert!(String::from_utf8_lossy(&shown.stdout).contains("failed   users.mixed#wantsAnError"), "{}", said(&shown));
    let rerun = project.run(&server.base_url(), &["--failed", "--reporter", "json"]).output().unwrap();
    assert_eq!(rerun.status.code(), Some(1), "{}", said(&rerun));
    assert_eq!(of_kind(&events(&rerun.stdout), "testFinished").len(), 1, "{}", said(&rerun));
}

/// A project whose last run failed only its parked test, run with `--include-skipped`.
fn parked_failure(base: &str) -> Project {
    let project = Project::new(&[CONFIG, USERS, PARKED]);
    project.run(base, &["users.parked#isParked", "--include-skipped"]).assert().code(1);
    project
}

/// When every failure is parked now, `--failed` has nothing to run: it exits 2 and suggests
/// `--include-skipped`.
#[tokio::test(flavor = "multi_thread")]
async fn failed_refuses_when_every_failure_is_parked() {
    let server = MockServer::start().await.unwrap();
    let project = parked_failure(&server.base_url());
    let parked = project.run(&server.base_url(), &["--failed"]).output().unwrap();
    assert_eq!(parked.status.code(), Some(2), "{}", said(&parked));
    assert!(said(&parked).contains("all marked `skip` now; nothing to run"), "{}", said(&parked));
    assert!(said(&parked).contains("`vc run --failed --include-skipped` reruns them anyway"), "{}", said(&parked));
}

/// `--failed --include-skipped` reruns a parked failure.
#[tokio::test(flavor = "multi_thread")]
async fn failed_with_include_skipped_reruns_a_parked_failure() {
    let server = MockServer::start().await.unwrap();
    let project = parked_failure(&server.base_url());
    let included = project.run(&server.base_url(), &["--failed", "--include-skipped"]).output().unwrap();
    assert_eq!(included.status.code(), Some(1), "{}", said(&included));
    assert!(said(&included).contains("✕ users.parked#isParked"), "{}", said(&included));
    assert!(said(&included).contains("0 passed · 1 failed"), "{}", said(&included));
}

/// When only some of the failures are parked, `--failed` reruns the rest and lists the parked
/// one as skipped.
#[tokio::test(flavor = "multi_thread")]
async fn failed_reruns_the_live_failures_and_lists_the_parked_one_as_skipped() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, PARKED]);
    project.run(&server.base_url(), &["--include-skipped"]).assert().code(1);
    let rerun = project.run(&server.base_url(), &["--failed"]).output().unwrap();
    assert_eq!(rerun.status.code(), Some(1), "{}", said(&rerun));
    for line in
        ["✕ users.parked#wantsAnError", "- users.parked#isParked skipped: later", "0 passed · 1 failed · 1 skipped"]
    {
        assert!(said(&rerun).contains(line), "{line}: {}", said(&rerun));
    }
}
