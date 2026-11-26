//! What the web UI's git pane reads and asks of version control (D546–D550): the branch and
//! the changed files, a file's diff, and a commit, push or pull. Never offered to agents.

use vc_model::SourcePath;

/// The branch and the files that differ from its last commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "git pane data built and read across crates; every user is in this repo (D160)"
)]
pub struct VcsStatus {
    /// The checked-out branch; empty when `HEAD` is detached.
    pub branch: String,
    /// The branch it tracks, such as `origin/main`; empty when it tracks none.
    pub upstream: String,
    /// Commits on the branch that its upstream lacks.
    pub ahead: u32,
    /// Commits on the upstream that the branch lacks.
    pub behind: u32,
    /// The changed files under the project root, by path.
    pub files: Vec<VcsFile>,
}

/// One changed file.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "git pane data built and read across crates; every user is in this repo (D160)"
)]
pub struct VcsFile {
    /// Where it is, relative to the project root.
    pub path: SourcePath,
    /// How it changed.
    pub change: VcsChange,
    /// Lines added since the last commit; every line of an untracked file.
    pub added: u32,
    /// Lines removed since the last commit.
    pub removed: u32,
    /// Whether it is binary, so has no line counts.
    pub binary: bool,
}

/// How a file changed since the last commit, staged or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(clippy::exhaustive_enums, reason = "closed set of git's change kinds (D546)")]
pub enum VcsChange {
    /// New to the repository and staged.
    Added,
    /// Changed.
    Modified,
    /// Removed.
    Deleted,
    /// New and not yet added; not ignored.
    Untracked,
    /// Left with merge conflicts.
    Conflicted,
}

impl VcsChange {
    /// Its name on the wire: `added`, `modified`, `deleted`, `untracked` or `conflicted`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
            Self::Untracked => "untracked",
            Self::Conflicted => "conflicted",
        }
    }
}

/// A commit of some changed files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "git pane data built and read across crates; every user is in this repo (D160)"
)]
pub struct VcsCommit {
    /// The files to commit, as they are now, relative to the project root; nothing else staged
    /// goes in.
    pub paths: Vec<SourcePath>,
    /// The commit message.
    pub message: String,
}

/// `text` with the user and password of every URL in it replaced by `***`, so that a remote URL
/// with credentials in it (`https://user:token@host/repo.git`) is never shown (D549).
#[must_use]
pub fn without_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let authority = tail.find(|c: char| c == '/' || c.is_whitespace() || "'\"<>".contains(c)).unwrap_or(tail.len());
        let (host, after) = tail.split_at(authority);
        match host.rfind('@') {
            Some(user_end) => {
                out.push_str("***");
                out.push_str(host.get(user_end..).unwrap_or_default());
            }
            None => out.push_str(host),
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::without_credentials;

    #[test]
    fn a_urls_user_and_password_are_hidden_and_the_rest_is_kept() {
        let said = "fatal: unable to access 'https://sam:ghp_x1Y2@github.com/o/r.git/': denied";
        assert_eq!(without_credentials(said), "fatal: unable to access 'https://***@github.com/o/r.git/': denied");
        let two = "from ssh://git@h.test/r and http://u:p@a.test";
        assert_eq!(without_credentials(two), "from ssh://***@h.test/r and http://***@a.test");
        for plain in ["To github.com:o/r.git", "https://github.com/o/r.git", "a :// b", ""] {
            assert_eq!(without_credentials(plain), plain, "{plain}");
        }
    }
}
