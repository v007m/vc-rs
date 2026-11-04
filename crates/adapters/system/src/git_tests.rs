use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use vc_interfaces::{Vcs, VcsError};
use vc_model::SourcePath;
use vc_testkit::{VcsHistory, vcs_suite};

use super::GitVcs;

fn git(root: &Path, args: &[&str]) {
    let identity = ["-c", "user.name=vc", "-c", "user.email=vc@example.test", "-c", "commit.gpgsign=false"];
    let status = Command::new("git").arg("-C").arg(root).args(identity).args(args).status().unwrap();
    assert!(status.success(), "git {args:?}");
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

/// A repository at `repo` holding `history` with the project in `folder` (`.` for the root).
fn repository(repo: &Path, folder: &str, history: &VcsHistory) {
    let project = repo.join(folder);
    git(repo, &["init", "-q", "-b", "main"]);
    write(&project, history.base);
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "base"]);
    git(repo, &["tag", VcsHistory::BASE]);
    write(&project, history.later);
    git(repo, &["commit", "-q", "-am", "later"]);
    write(&project, history.edited);
    write(&project, history.untracked);
    write(&project, history.ignored);
}

#[test]
fn meets_the_vcs_contract() {
    let repo = TempDir::new().unwrap();
    vcs_suite(&mut |history| {
        repository(repo.path(), ".", history);
        Box::new(GitVcs::new(repo.path())) as Box<dyn Vcs>
    });
}

#[test]
fn paths_are_relative_to_a_project_inside_the_repository_and_files_outside_it_are_left_out() {
    let repo = TempDir::new().unwrap();
    let history = VcsHistory::STANDARD;
    repository(repo.path(), "api", &history);
    write(repo.path(), &[("web/app.ts", "x\n"), ("api.test.yaml", "tests: []\n")]);
    let changed = GitVcs::new(repo.path().join("api")).changed(VcsHistory::BASE).unwrap();
    assert_eq!(changed, history.changed());
}

#[test]
fn a_staged_change_and_a_deleted_file_count_as_changed() {
    let repo = TempDir::new().unwrap();
    repository(repo.path(), ".", &VcsHistory::STANDARD);
    git(repo.path(), &["add", "services/users/crud.test.yaml"]);
    git(repo.path(), &["rm", "-q", "services/users/users.api.yaml"]);
    let changed = GitVcs::new(repo.path()).changed(VcsHistory::BASE).unwrap();
    assert!(changed.contains(&SourcePath::new("services/users/crud.test.yaml").unwrap()));
    assert!(changed.contains(&SourcePath::new("services/users/users.api.yaml").unwrap()));
}

#[test]
fn outside_a_repository_it_fails_with_gits_message() {
    let folder = TempDir::new().unwrap();
    let Err(VcsError::Failed { base, reason }) = GitVcs::new(folder.path()).changed("HEAD") else {
        panic!("expected a failure outside a repository");
    };
    assert_eq!(base, "HEAD");
    assert_ne!(reason.trim(), "", "git explains the failure");
}
