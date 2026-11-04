//! The version control the engine asks which files changed, for `vc run --changed` (D130).

use vc_model::SourcePath;

/// Lists the project's files that changed since a base ref (D130, D133).
///
/// The engine maps the paths to file IDs ([`SourcePath::file_id`]) for the planner's
/// `Selection::changed`.
pub trait Vcs: Send + Sync {
    /// The files that differ from `base` (committed since it, staged or only in the working tree,
    /// deleted ones included) and the untracked files that are not ignored, relative to the
    /// project root, sorted and without repeats. Files outside the project root are left out.
    ///
    /// # Errors
    ///
    /// [`VcsError::InvalidRef`] when `base` is not a plain ref ([`checked_ref`]),
    /// [`VcsError::Unavailable`] when there is no tool to ask, and [`VcsError::Failed`] when it
    /// fails, as outside a repository or on a ref that does not exist.
    fn changed(&self, base: &str) -> Result<Vec<SourcePath>, VcsError>;
}

/// `base` when it is a plain ref such as `origin/main`, `HEAD~1` or `main@{1}`.
///
/// A plain ref is at least one of letters, digits and `/ . _ - + ~ ^ @ { }`, does not start with
/// `-` and holds no `..`. Every [`Vcs`] checks the ref with it first, so a ref can never be read
/// as an option or a range.
///
/// # Errors
///
/// [`VcsError::InvalidRef`] for any other text.
pub fn checked_ref(base: &str) -> Result<&str, VcsError> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || "/._-+~^@{}".contains(c);
    let plain = !base.is_empty() && !base.starts_with('-') && !base.contains("..") && base.chars().all(allowed);
    if plain { Ok(base) } else { Err(VcsError::InvalidRef { base: base.to_owned() }) }
}

/// Why the changed files could not be listed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum VcsError {
    /// `base` is empty, starts with `-` or holds a character a ref name cannot.
    #[error("{base:?} is not a ref name such as `origin/main` or `HEAD~1`")]
    InvalidRef {
        /// The ref as written.
        base: String,
    },
    /// There is no tool to ask, such as no `git` on the `PATH`.
    #[error("cannot ask version control which files changed: {reason}")]
    Unavailable {
        /// What went wrong.
        reason: String,
    },
    /// The tool ran and failed, as outside a repository or on a ref that does not exist.
    #[error("cannot list the files changed since {base}: {reason}")]
    Failed {
        /// The ref compared against.
        base: String,
        /// The tool's own message.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{VcsError, checked_ref};

    #[test]
    fn plain_refs_pass_and_options_ranges_and_odd_characters_do_not() {
        for good in ["origin/main", "HEAD", "HEAD~1", "HEAD^", "v2.0", "main@{1}", "feat/a_b-c+d", "3f2a9c1"] {
            assert_eq!(checked_ref(good), Ok(good), "{good}");
        }
        for bad in ["", "-p", "--output=/tmp/x", "a..b", "a...b", "a b", "a;b", "$(id)", "HEAD:x", "a\nb", "`x`"] {
            assert_eq!(checked_ref(bad), Err(VcsError::InvalidRef { base: bad.to_owned() }), "{bad:?}");
        }
    }
}
