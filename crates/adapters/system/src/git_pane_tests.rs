use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use tempfile::TempDir;
use vc_interfaces::{Vcs, VcsChange, VcsCommit, VcsError, VcsFile, VcsStatus};
use vc_model::SourcePath;

use super::{git, write};
use crate::GitVcs;

fn path(text: &str) -> SourcePath {
    SourcePath::new(text).unwrap()
}

fn changed(text: &str, change: VcsChange, (added, removed): (u32, u32)) -> VcsFile {
    VcsFile { path: path(text), change, added, removed, binary: false }
}

/// A repository on `main` whose own config can commit, as the git pane does.
fn repository(repo: &Path) {
    git(repo, &["init", "-q", "-b", "main"]);
    let config = [("user.name", "vc"), ("user.email", "vc@example.test"), ("commit.gpgsign", "false")];
    for (key, value) in config.into_iter().chain([("push.autoSetupRemote", "false"), ("push.default", "simple")]) {
        git(repo, &["config", key, value]);
    }
}

/// What `git args` printed in `repo`.
fn output(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git").arg("-C").arg(repo).args(args).output().unwrap();
    String::from_utf8(output.stdout).unwrap()
}

/// A committed project with every kind of change but a conflict.
fn changes(repo: &Path) {
    repository(repo);
    write(repo, &[(".gitignore", "*.log\n"), ("a.yaml", "one\ntwo\nthree\n"), ("b.yaml", "gone\n")]);
    write(repo, &[("logo.bin", "\0\x01")]);
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "start"]);
    write(repo, &[("a.yaml", "one\n2\nthree\nfour\n"), ("c.yaml", "new\n"), ("d.yaml", "x\ny\nz"), ("e.log", "no\n")]);
    write(repo, &[("logo.bin", "\0\x02")]);
    std::fs::remove_file(repo.join("b.yaml")).unwrap();
    git(repo, &["add", "c.yaml"]);
}

#[test]
fn the_status_names_the_branch_and_each_changed_file_with_its_kind_and_line_counts() {
    let repo = TempDir::new().unwrap();
    changes(repo.path());
    let status = GitVcs::new(repo.path()).status().unwrap();
    let binary = VcsFile { binary: true, ..changed("logo.bin", VcsChange::Modified, (0, 0)) };
    let files = vec![
        changed("a.yaml", VcsChange::Modified, (2, 1)),
        changed("b.yaml", VcsChange::Deleted, (0, 1)),
        changed("c.yaml", VcsChange::Added, (1, 0)),
        changed("d.yaml", VcsChange::Untracked, (3, 0)),
        binary,
    ];
    let expected = VcsStatus { branch: "main".to_owned(), files, ..VcsStatus::default() };
    assert_eq!(status, expected);
}

#[test]
fn the_status_of_a_project_inside_the_repository_leaves_out_the_files_outside_it() {
    let repo = TempDir::new().unwrap();
    repository(repo.path());
    write(repo.path(), &[("api/a.yaml", "a\n"), ("web/app.ts", "x\n")]);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "start"]);
    write(repo.path(), &[("api/a.yaml", "b\n"), ("web/app.ts", "y\n"), ("api/n.yaml", "n\n"), ("top.yaml", "t\n")]);
    let status = GitVcs::new(repo.path().join("api")).status().unwrap();
    let files = vec![changed("a.yaml", VcsChange::Modified, (1, 1)), changed("n.yaml", VcsChange::Untracked, (1, 0))];
    assert_eq!(status.files, files);
}

#[test]
fn before_the_first_commit_staged_files_are_added_and_the_rest_untracked() {
    let repo = TempDir::new().unwrap();
    repository(repo.path());
    write(repo.path(), &[("a.yaml", "a\nb\n"), ("b.yaml", "c\n")]);
    git(repo.path(), &["add", "a.yaml"]);
    let status = GitVcs::new(repo.path()).status().unwrap();
    assert_eq!(status.branch, "main");
    let files = vec![changed("a.yaml", VcsChange::Added, (2, 0)), changed("b.yaml", VcsChange::Untracked, (1, 0))];
    assert_eq!(status.files, files);
}

#[test]
fn a_file_left_with_merge_conflicts_is_conflicted() {
    let repo = TempDir::new().unwrap();
    repository(repo.path());
    write(repo.path(), &[("a.yaml", "base\n")]);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "start"]);
    git(repo.path(), &["checkout", "-q", "-b", "other"]);
    write(repo.path(), &[("a.yaml", "theirs\n")]);
    git(repo.path(), &["commit", "-q", "-am", "theirs"]);
    git(repo.path(), &["checkout", "-q", "main"]);
    write(repo.path(), &[("a.yaml", "ours\n")]);
    git(repo.path(), &["commit", "-q", "-am", "ours"]);
    let merged = Command::new("git").arg("-C").arg(repo.path()).args(["merge", "-q", "other"]).output().unwrap();
    assert!(!merged.status.success(), "the merge conflicts");
    let status = GitVcs::new(repo.path()).status().unwrap();
    assert_eq!(status.files.iter().map(|file| (file.path.as_str(), file.change)).collect::<Vec<_>>(), [(
        "a.yaml",
        VcsChange::Conflicted
    )]);
}

#[test]
fn a_diff_shows_a_changed_file_against_the_last_commit_and_an_untracked_one_whole() {
    let repo = TempDir::new().unwrap();
    changes(repo.path());
    let vcs = GitVcs::new(repo.path());
    let modified = vcs.diff(&path("a.yaml")).unwrap();
    assert!(modified.contains("--- a/a.yaml\n+++ b/a.yaml\n"), "{modified}");
    assert!(modified.contains("-two\n+2\n") && modified.contains("+four\n"), "{modified}");
    let untracked = vcs.diff(&path("d.yaml")).unwrap();
    assert!(untracked.contains("--- /dev/null\n+++ b/d.yaml\n"), "{untracked}");
    assert!(untracked.contains("+x\n+y\n+z\n"), "{untracked}");
    assert!(vcs.diff(&path(".gitignore")).unwrap().is_empty(), "an unchanged file has no diff");
    assert!(vcs.diff(&path("e.log")).unwrap().is_empty(), "an ignored file has no diff");
}

#[test]
fn a_commit_takes_the_named_files_as_they_are_and_leaves_the_rest_staged_or_not() {
    let repo = TempDir::new().unwrap();
    changes(repo.path());
    let commit = VcsCommit { paths: vec![path("a.yaml"), path("b.yaml"), path("d.yaml")], message: "pane".to_owned() };
    let id = GitVcs::new(repo.path()).commit(&commit).unwrap();
    assert_eq!(id, output(repo.path(), &["rev-parse", "--short", "HEAD"]).trim());
    assert_eq!(output(repo.path(), &["log", "-1", "--format=%s"]), "pane\n");
    let committed = output(repo.path(), &["diff-tree", "--no-commit-id", "--name-status", "-r", "HEAD"]);
    assert_eq!(committed, "M\ta.yaml\nD\tb.yaml\nA\td.yaml\n");
    assert_eq!(output(repo.path(), &["status", "--porcelain"]), "A  c.yaml\n M logo.bin\n");
}

#[test]
fn a_commit_with_no_message_or_no_files_is_refused_and_commits_nothing() {
    let repo = TempDir::new().unwrap();
    changes(repo.path());
    let vcs = GitVcs::new(repo.path());
    for commit in [
        VcsCommit { paths: vec![path("a.yaml")], message: " \n".to_owned() },
        VcsCommit { paths: Vec::new(), message: "nothing".to_owned() },
    ] {
        let refused = vcs.commit(&commit);
        assert!(matches!(&refused, Err(VcsError::Action { action, .. }) if action == "commit"), "{refused:?}");
    }
    let missing = vcs.commit(&VcsCommit { paths: vec![path("none.yaml")], message: "m".to_owned() });
    assert!(matches!(missing, Err(VcsError::Action { .. })), "{missing:?}");
    assert_eq!(output(repo.path(), &["rev-list", "--count", "HEAD"]), "1\n");
}

/// `repo` and a second clone, both tracking the bare `remote`.
fn remote(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let (bare, repo, other) = (root.join("remote.git"), root.join("repo"), root.join("other"));
    git(root, &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()]);
    std::fs::create_dir_all(&repo).unwrap();
    repository(&repo);
    write(&repo, &[("a.yaml", "a\n")]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "start"]);
    git(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&repo, &["push", "-q", "-u", "origin", "main"]);
    git(root, &["clone", "-q", bare.to_str().unwrap(), other.to_str().unwrap()]);
    (repo, other)
}

#[test]
fn a_push_sends_the_commits_ahead_and_a_pull_fast_forwards_the_ones_behind() {
    let root = TempDir::new().unwrap();
    let (repo, other) = remote(root.path());
    let vcs = GitVcs::new(&repo);
    write(&repo, &[("b.yaml", "b\n")]);
    vcs.commit(&VcsCommit { paths: vec![path("b.yaml")], message: "b".to_owned() }).unwrap();
    let ahead = vcs.status().unwrap();
    assert_eq!((ahead.upstream.as_str(), ahead.ahead, ahead.behind), ("origin/main", 1, 0));
    vcs.push().unwrap();
    assert_eq!(vcs.status().unwrap().ahead, 0);
    git(&other, &["pull", "-q"]);
    write(&other, &[("c.yaml", "c\n")]);
    git(&other, &["add", "-A"]);
    git(&other, &["commit", "-q", "-m", "c"]);
    git(&other, &["push", "-q"]);
    git(&repo, &["fetch", "-q"]);
    assert_eq!(vcs.status().unwrap().behind, 1);
    vcs.pull().unwrap();
    assert_eq!(vcs.status().unwrap().behind, 0);
    assert!(repo.join("c.yaml").exists());
}

#[test]
fn a_pull_that_is_no_fast_forward_is_refused_and_a_push_with_no_upstream_fails() {
    let root = TempDir::new().unwrap();
    let (repo, other) = remote(root.path());
    write(&other, &[("c.yaml", "c\n")]);
    git(&other, &["add", "-A"]);
    git(&other, &["commit", "-q", "-m", "c"]);
    git(&other, &["push", "-q"]);
    let vcs = GitVcs::new(&repo);
    write(&repo, &[("b.yaml", "b\n")]);
    vcs.commit(&VcsCommit { paths: vec![path("b.yaml")], message: "b".to_owned() }).unwrap();
    let pulled = vcs.pull();
    assert!(matches!(&pulled, Err(VcsError::Action { action, .. }) if action == "pull"), "{pulled:?}");
    assert!(!repo.join("c.yaml").exists(), "nothing was merged");
    git(&repo, &["checkout", "-q", "-b", "alone"]);
    let pushed = vcs.push();
    assert!(matches!(&pushed, Err(VcsError::Action { action, .. }) if action == "push"), "{pushed:?}");
}

#[test]
fn a_failed_push_never_shows_the_credentials_in_the_remote_url() {
    let repo = TempDir::new().unwrap();
    repository(repo.path());
    write(repo.path(), &[("a.yaml", "a\n")]);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "start"]);
    git(repo.path(), &["remote", "add", "origin", "http://sam:s3cr3t-token@127.0.0.1:9/r.git"]);
    git(repo.path(), &["config", "branch.main.remote", "origin"]);
    git(repo.path(), &["config", "branch.main.merge", "refs/heads/main"]);
    let vcs = GitVcs::new(repo.path());
    for result in [vcs.push(), vcs.pull()] {
        let Err(VcsError::Action { reason, .. }) = result else { panic!("expected a failure: {result:?}") };
        assert!(!reason.contains("s3cr3t") && !reason.contains("sam:"), "{reason}");
    }
}

#[test]
fn outside_a_repository_the_status_fails_with_gits_message() {
    let folder = TempDir::new().unwrap();
    let status = GitVcs::new(folder.path()).status();
    assert!(matches!(&status, Err(VcsError::Action { action, reason }) if action == "read the status" && !reason.is_empty()), "{status:?}");
}

#[test]
fn a_push_still_running_at_its_limit_is_killed_and_says_it_may_wait_for_a_passphrase() {
    let repo = TempDir::new().unwrap();
    repository(repo.path());
    write(repo.path(), &[("a.yaml", "one\n")]);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "start"]);
    git(repo.path(), &["remote", "add", "origin", "ssh://vc.invalid/r.git"]);
    git(repo.path(), &["config", "core.sshCommand", "sh -c 'sleep 30'"]);
    let started = Instant::now();
    let pushed = crate::git_pane::timed(repo.path(), ("push", &["push", "origin", "main"]), Duration::from_secs(1));
    assert!(started.elapsed() < Duration::from_secs(10), "stopped at its limit, not after the 30 s an ssh waits");
    let Err(VcsError::Action { action, reason }) = pushed else { panic!("{pushed:?}") };
    assert_eq!(action, "push");
    let said = "git push timed out after 1 s; it may be waiting for an SSH passphrase. Run it in a terminal or add the key to ssh-agent";
    assert_eq!(reason, said);
}
