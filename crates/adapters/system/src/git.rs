//! The `git` command line as the project's version control (D130, D133).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Output};

use vc_interfaces::{Vcs, VcsError};
use vc_model::SourcePath;

/// A [`Vcs`] that runs the `git` on the `PATH` in the project root.
///
/// Every argument is passed on its own, never through a shell, and the base ref is checked
/// with [`vc_interfaces::checked_ref`] and placed after `--end-of-options`, so it cannot be read
/// as an option. `core.fsmonitor` is turned off, so asking runs no hook the repository names.
/// The base is first resolved with `rev-parse --verify <base>^{commit}`: outside a repository
/// `git diff` would otherwise compare two files on disk (`--no-index`). Paths are relative to the
/// project root, which may be a folder inside the repository; files outside it are left out.
#[derive(Clone, Debug)]
pub struct GitVcs {
    root: PathBuf,
}

impl GitVcs {
    /// Asks the repository that holds the project at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The `\0`-separated paths `git args` prints, or why it could not.
    fn paths(&self, base: &str, args: &[&str]) -> Result<Vec<String>, VcsError> {
        let mut command = Command::new("git");
        command.arg("-C").arg(&self.root).args(["-c", "core.fsmonitor=false", "-c", "core.quotePath=false"]);
        let output =
            command.args(args).output().map_err(|error| VcsError::Unavailable { reason: error.to_string() })?;
        listed(base, &output)
    }
}

impl Vcs for GitVcs {
    fn changed(&self, base: &str) -> Result<Vec<SourcePath>, VcsError> {
        let base = vc_interfaces::checked_ref(base)?;
        let commit = format!("{base}^{{commit}}");
        self.paths(base, &["rev-parse", "--verify", &commit])?;
        let diff = ["diff", "--name-only", "-z", "--no-renames", "--relative", "--end-of-options", base, "--"];
        let mut paths = self.paths(base, &diff)?;
        paths.extend(self.paths(base, &["ls-files", "--others", "--exclude-standard", "-z", "--"])?);
        let paths: BTreeSet<SourcePath> = paths.iter().filter_map(|path| SourcePath::new(path).ok()).collect();
        Ok(paths.into_iter().collect())
    }
}

/// The paths in `output`, or a [`VcsError::Failed`] with git's message when it failed.
fn listed(base: &str, output: &Output) -> Result<Vec<String>, VcsError> {
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(VcsError::Failed { base: base.to_owned(), reason });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.split('\0').filter(|path| !path.is_empty()).map(str::to_owned).collect())
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
