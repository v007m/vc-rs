use std::fs;
use std::path::Path;
use std::process::Command;

use vc_contracts::engine::v1 as wire;

use super::{commit, diff, pull, push, status};
use crate::Code;
use crate::engine_fixture::{CONFIG, SECRET, project};

/// The settings each test repository uses, the same on every platform: no CRLF checkout on Windows.
const IDENTITY: [(&str, &str); 4] =
    [("user.name", "vc"), ("user.email", "vc@example.test"), ("commit.gpgsign", "false"), ("core.autocrlf", "false")];

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git").arg("-C").arg(root).args(args).output().unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// The fixture project committed in a repository of its own, then its config changed.
fn committed(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    for (key, value) in IDENTITY {
        git(root, &["config", key, value]);
    }
    fs::write(root.join(".gitignore"), ".env\n.vibranium/\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "start"]);
    fs::write(root.join("vibranium.config.yaml"), CONFIG.replace("project: shop", "project: store")).unwrap();
    fs::write(root.join("notes.txt"), "one\ntwo\n").unwrap();
}

#[tokio::test]
async fn the_status_lists_each_changed_file_and_a_diff_leaves_masked() {
    let (dir, api) = project();
    committed(dir.path());
    let shown = status(api.clone(), wire::VcsStatusRequest {}).await.unwrap();
    assert_eq!((shown.branch.as_str(), shown.upstream.as_str(), shown.ahead, shown.behind), ("main", "", 0, 0));
    let files: Vec<_> =
        shown.files.iter().map(|file| (file.path.as_str(), file.change(), file.added, file.removed)).collect();
    assert_eq!(
        files,
        [("notes.txt", wire::VcsChange::Untracked, 2, 0), ("vibranium.config.yaml", wire::VcsChange::Modified, 1, 1),]
    );
    let config = diff(api.clone(), wire::VcsDiffRequest { path: "vibranium.config.yaml".into() }).await.unwrap();
    assert!(config.diff.contains("-project: shop\n+project: store\n"), "{}", config.diff);
    assert!(!config.diff.contains(SECRET), "the context line is masked: {}", config.diff);
    let outside = diff(api, wire::VcsDiffRequest { path: "../x".into() }).await.unwrap_err();
    assert_eq!(outside.code, Code::InvalidArgument, "{outside:?}");
}

#[tokio::test]
async fn a_commit_takes_only_the_named_files_and_a_push_with_no_upstream_fails_its_precondition() {
    let (dir, api) = project();
    committed(dir.path());
    let request = wire::VcsCommitRequest { paths: vec!["vibranium.config.yaml".into()], message: "rename".into() };
    let made = commit(api.clone(), request).await.unwrap();
    assert_eq!(made.commit, git(dir.path(), &["rev-parse", "--short", "HEAD"]).trim());
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "?? notes.txt\n");
    let blank = wire::VcsCommitRequest { paths: vec!["notes.txt".into()], message: " ".into() };
    assert_eq!(commit(api.clone(), blank).await.unwrap_err().code, Code::FailedPrecondition);
    let pushed = push(api, wire::VcsPushRequest {}).await.unwrap_err();
    assert_eq!(pushed.code, Code::FailedPrecondition, "{pushed:?}");
    assert!(pushed.message.starts_with("cannot push: "), "{pushed:?}");
}

#[tokio::test]
async fn outside_a_repository_the_status_fails_its_precondition() {
    let (_dir, api) = project();
    let refused = status(api, wire::VcsStatusRequest {}).await;
    assert!(matches!(&refused, Err(error) if error.code == Code::FailedPrecondition), "{refused:?}");
}

/// `root` committed, with an `origin` remote in `remotes` it tracks and a clone of that remote,
/// which is returned.
fn tracked(root: &Path, remotes: &Path) -> std::path::PathBuf {
    committed(root);
    let (bare, other) = (remotes.join("origin.git"), remotes.join("other"));
    git(remotes, &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()]);
    git(root, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(root, &["push", "-q", "-u", "origin", "main"]);
    git(remotes, &["clone", "-q", bare.to_str().unwrap(), other.to_str().unwrap()]);
    for (key, value) in IDENTITY {
        git(&other, &["config", key, value]);
    }
    other
}

#[tokio::test]
async fn a_push_sends_the_commit_upstream_and_a_pull_fast_forwards_what_another_clone_pushed() {
    let (dir, api) = project();
    let remotes = tempfile::TempDir::new().unwrap();
    let other = tracked(dir.path(), remotes.path());
    let request = wire::VcsCommitRequest { paths: vec!["notes.txt".into()], message: "notes".into() };
    commit(api.clone(), request).await.unwrap();
    assert_eq!(status(api.clone(), wire::VcsStatusRequest {}).await.unwrap().ahead, 1);
    push(api.clone(), wire::VcsPushRequest {}).await.unwrap();
    let shown = status(api.clone(), wire::VcsStatusRequest {}).await.unwrap();
    assert_eq!((shown.upstream.as_str(), shown.ahead), ("origin/main", 0));
    git(&other, &["pull", "-q"]);
    fs::write(other.join("more.txt"), "more\n").unwrap();
    git(&other, &["add", "more.txt"]);
    git(&other, &["commit", "-q", "-m", "more"]);
    git(&other, &["push", "-q"]);
    pull(api, wire::VcsPullRequest {}).await.unwrap();
    assert_eq!(fs::read_to_string(dir.path().join("more.txt")).unwrap(), "more\n");
}

#[tokio::test]
async fn a_failed_push_or_pull_is_a_failed_precondition_whose_message_holds_no_credentials() {
    let (dir, api) = project();
    committed(dir.path());
    git(dir.path(), &["remote", "add", "origin", "http://sam:s3cr3t-token@127.0.0.1:9/r.git"]);
    git(dir.path(), &["config", "branch.main.remote", "origin"]);
    git(dir.path(), &["config", "branch.main.merge", "refs/heads/main"]);
    let pushed = push(api.clone(), wire::VcsPushRequest {}).await.unwrap_err();
    let pulled = pull(api, wire::VcsPullRequest {}).await.unwrap_err();
    for (refused, said) in [(pushed, "cannot push: "), (pulled, "cannot pull: ")] {
        assert_eq!(refused.code, Code::FailedPrecondition, "{refused:?}");
        assert!(refused.message.starts_with(said), "{refused:?}");
        assert!(!refused.message.contains("s3cr3t") && !refused.message.contains("sam:"), "{refused:?}");
    }
}

#[tokio::test]
async fn a_commit_of_a_path_outside_the_project_or_of_no_path_is_refused_and_commits_nothing() {
    let (dir, api) = project();
    committed(dir.path());
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let outside = wire::VcsCommitRequest { paths: vec!["../x".into()], message: "m".into() };
    assert_eq!(commit(api.clone(), outside).await.unwrap_err().code, Code::InvalidArgument);
    let none = wire::VcsCommitRequest { paths: Vec::new(), message: "m".into() };
    assert_eq!(commit(api, none).await.unwrap_err().code, Code::FailedPrecondition);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
}
