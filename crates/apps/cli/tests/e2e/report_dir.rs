//! Where a run writes its reports (D327): `reports.dir` in the project config stays inside the
//! project (V2313, and V5121 for one a link leads out of), and so does `VIB_REPORTS_DIR` in the
//! project's `.env`, while a folder the user chooses, with `--out`, `--set` or `VIB_REPORTS_DIR` in
//! the environment, may be anywhere.

use std::fs;
use std::path::Path;

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, USERS, said};

const LISTS: (&str, &str) = ("services/users/lists.test.yaml", "tests:\n  - id: listsUsers\n    call: users#list\n");

/// The names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn a_config_reports_dir_that_leaves_the_project_is_refused() {
    for dir in ["/tmp/vc-reports", "../ci-reports", "reports/../../ci"] {
        let config = format!("{}reports: {{dir: '{dir}'}}\n", CONFIG.1);
        let project = Project::new(&[(CONFIG.0, config.as_str()), USERS, LISTS]);
        let check = project.vc(&["check"]).output().unwrap();
        let printed = said(&check);
        assert_eq!(check.status.code(), Some(2), "{printed}");
        assert!(printed.contains("error[V2313]") && printed.contains("leaves the project"), "{dir}: {printed}");
        assert!(printed.contains("vibranium.config.yaml:6") && printed.contains("--out <dir>"), "{printed}");
    }
    let config = format!("{}reports: {{dir: reports/ci}}\n", CONFIG.1);
    let project = Project::new(&[(CONFIG.0, config.as_str()), USERS, LISTS]);
    project.vc(&["check"]).assert().code(0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_the_user_chooses_may_be_outside_the_project() {
    let server = MockServer::start().await.unwrap();
    let (out, set) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let project = Project::new(&[CONFIG, USERS, LISTS]);
    let out_dir = out.path().to_string_lossy().into_owned();
    project.run(&server.base_url(), &["--out", &out_dir]).assert().code(0);
    assert!(out.path().join("ctrf.json").is_file(), "`--out` writes straight into its folder");
    let set_dir = format!("reports.dir={}", set.path().display());
    project.run(&server.base_url(), &["--set", &set_dir]).assert().code(0);
    assert!(
        set.path().join("2/ctrf.json").is_file(),
        "`--set reports.dir` gets a folder per run: {:?}",
        names(set.path())
    );
    let env = tempfile::tempdir().unwrap();
    project.run(&server.base_url(), &[]).env("VIB_REPORTS_DIR", env.path()).assert().code(0);
    assert!(env.path().join("3/ctrf.json").is_file(), "`VIB_REPORTS_DIR` too: {:?}", names(env.path()));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_report_folder_linked_out_of_the_project_gets_nothing_and_is_never_pruned() {
    let server = MockServer::start().await.unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(outside.path().join("1")).unwrap();
    fs::write(outside.path().join("1/.vc-run"), "").unwrap();
    let project = Project::new(&[CONFIG, USERS, LISTS]);
    fs::create_dir(project.path().join(".vibranium")).unwrap();
    std::os::unix::fs::symlink(outside.path(), project.path().join(".vibranium/reports")).unwrap();
    for _ in 0..2 {
        let run = project.run(&server.base_url(), &["--report", "ctrf,junit,html"]).output().unwrap();
        let printed = said(&run);
        assert_eq!(run.status.code(), Some(0), "the tests still pass: {printed}");
        assert!(printed.contains("V5121") && printed.contains("no report is written"), "{printed}");
    }
    assert_eq!(names(outside.path()), ["1"], "nothing is written through the link");
    let prune = project.vc(&["history", "prune", "--keep", "1"]).output().unwrap();
    let printed = said(&prune);
    assert_eq!(prune.status.code(), Some(2), "{printed}");
    assert!(printed.contains("V5121") && printed.contains("no run is pruned"), "{printed}");
    assert!(outside.path().join("1/.vc-run").is_file(), "a marked folder through the link stays");
    let dir = outside.path().to_string_lossy().into_owned();
    let pruned = project.vc(&["history", "prune", "--keep", "1"]).env("VIB_REPORTS_DIR", &dir).assert().code(0);
    pruned.stdout("pruned 1 run\n");
    assert!(!outside.path().join("1").exists(), "a folder the user chose is theirs to prune");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_report_folder_the_project_env_file_points_outside_gets_nothing_and_is_never_pruned() {
    let server = MockServer::start().await.unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(outside.path().join("1")).unwrap();
    fs::write(outside.path().join("1/.vc-run"), "").unwrap();
    let sibling = outside.path().file_name().unwrap().to_string_lossy().into_owned();
    for dir in [format!("../{sibling}"), outside.path().display().to_string()] {
        let env = format!("VIB_REPORTS_DIR={dir}\n");
        let project = Project::new(&[CONFIG, USERS, LISTS, (".env", env.as_str())]);
        let run = project.run(&server.base_url(), &["--report", "ctrf,junit,html"]).output().unwrap();
        let printed = said(&run);
        assert_eq!(run.status.code(), Some(0), "the tests still pass: {printed}");
        assert!(printed.contains("V5121") && printed.contains("no report is written"), "{dir}: {printed}");
        assert_eq!(names(outside.path()), ["1"], "{dir}: a cloned `.env` writes nothing outside");
        let prune = project.vc(&["history", "prune", "--keep", "0"]).output().unwrap();
        let printed = said(&prune);
        assert_eq!(prune.status.code(), Some(2), "{printed}");
        assert!(printed.contains("V5121") && printed.contains("no run is pruned"), "{dir}: {printed}");
        assert!(outside.path().join("1/.vc-run").is_file(), "{dir}: a marked folder outside stays");
    }
}
