use std::fs;
use std::path::Path;
use std::process::Command;

use vc_contracts::engine::v1 as wire;

use super::{commit, diff, push, status};
use crate::Code;
use crate::engine_fixture::{CONFIG, SECRET, project};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git").arg("-C").arg(root).args(args).output().unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// The fixture project committed in a repository of its own, then its config changed.
fn committed(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    for (key, value) in [("user.name", "vc"), ("user.email", "vc@example.test"), ("commit.gpgsign", "false")] {
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
