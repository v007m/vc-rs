//! Confining a plugin's process to what its manifest declares (D254), where the OS can: Landlock
//! on Linux, through `vc` run again as a launcher ([`LAUNCHER_ARG`]) that confines itself and
//! then becomes the plugin, and Seatbelt (`sandbox-exec`) on macOS. Where a plugin cannot be
//! confined ([`ready`]), it runs unconfined with a warning, but on macOS it is not started unless
//! the user allows that ([`FAIL_CLOSED`], D260).
//!
//! The project folder is closed to the plugin but for its install folder, which it may read, and
//! its data folder and the project files its manifest names, which it may read and write. Outside
//! the project it may read anything, and write only the temporary folder (not when the project is
//! inside it) and [`DEVICES`]. A plugin that names no network host gets no network (on Linux, no
//! TCP); one that names any may reach every host, since neither OS filters by host name.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use vc_interfaces::PluginPermissions;

#[cfg(target_os = "linux")]
use crate::confine_linux as platform;
#[cfg(target_os = "macos")]
use crate::confine_macos as platform;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use declared as platform;

/// The first argument that makes `vc` the plugin launcher: `vc <LAUNCHER_ARG> <policy JSON>
/// <program> <args>…`.
pub const LAUNCHER_ARG: &str = "__vc-plugin-launcher";

/// The device files a plugin may write outside its folders.
pub const DEVICES: [&str; 2] = ["/dev/null", "/dev/tty"];

/// A program and its arguments.
pub type Entry = (PathBuf, Vec<OsString>);

/// Whether a plugin that cannot be confined is refused rather than run unconfined: on macOS,
/// where `sandbox-exec` is always there to confine it (D260).
pub const FAIL_CLOSED: bool = platform::FAIL_CLOSED;

/// The files a [`Policy`] lets its plugin write that the OS cannot let it make while they do not
/// exist: on Linux, those at the top of the project, as Landlock grants only what exists and the
/// project root is never granted; none elsewhere. It reads the file system, so call it off the
/// runtime's workers.
pub use platform::uncreatable;

/// Whether a plugin can be confined here, with `launcher` (`vc`) as its launcher; why not
/// otherwise. It may run a program, so call it off the runtime's workers.
///
/// # Errors
///
/// Why it cannot: Linux without a launcher or Landlock, macOS where `sandbox-exec` cannot start,
/// any other OS.
pub fn ready(launcher: Option<&Path>) -> Result<(), String> {
    platform::ready(launcher)
}

/// How this process confines plugins: with `vc` as their launcher on Linux, if any; and whether
/// the OS can confine them here ([`ready`]), learned once and shared by every host given it.
#[derive(Debug, Default)]
pub struct Confiner {
    launcher: Option<PathBuf>,
    ready: OnceCell<Result<(), String>>,
}

impl Confiner {
    /// A confiner starting plugins through `launcher` (`vc`) on Linux; none there leaves them
    /// unconfined.
    #[must_use]
    pub fn new(launcher: Option<PathBuf>) -> Self {
        Self { launcher, ready: OnceCell::new() }
    }

    /// Whether a plugin can be confined here; why not otherwise. The first call asks the OS, off
    /// the runtime's workers; later ones reuse its answer.
    ///
    /// # Errors
    ///
    /// Why it cannot, as [`ready`] says.
    pub async fn ready(&self) -> Result<(), String> {
        let launcher = self.launcher.clone();
        let probe = || async move {
            let probed = tokio::task::spawn_blocking(move || ready(launcher.as_deref())).await;
            probed.unwrap_or_else(|error| Err(error.to_string()))
        };
        self.ready.get_or_init(probe).await.clone()
    }

    /// What to run so that `entry` runs confined to `policy` ([`Policy::entry`]).
    ///
    /// # Errors
    ///
    /// Why not, as [`Policy::entry`] says.
    pub fn entry(&self, policy: &Policy, entry: Entry) -> Result<Entry, String> {
        policy.entry(self.launcher.as_deref(), entry)
    }
}

/// What a plugin's process may touch: real paths, links resolved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// The project folder, closed but for the paths below.
    pub root: PathBuf,
    /// Paths it may read (its install folder).
    pub read: Vec<PathBuf>,
    /// Paths it may read and write.
    pub write: Vec<PathBuf>,
    /// Whether it may use the network.
    pub network: bool,
}

#[cfg(test)]
impl Policy {
    /// A policy for the platform tests: `root` closed but for `read`, which it may read, and
    /// `write`, which it may write; no network.
    pub fn closed(root: &Path, read: &Path, write: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            read: vec![read.to_path_buf()],
            write: vec![write.to_path_buf()],
            network: false,
        }
    }
}

impl Policy {
    /// The policy of a plugin installed in `dir` in the project at `root`, asking for
    /// `permissions`; why not when a file it names is not a path inside the project.
    pub fn new(root: &Path, dir: &Path, permissions: &PluginPermissions) -> Result<Self, String> {
        let root = real(root);
        let mut write: Vec<PathBuf> = DEVICES.iter().map(PathBuf::from).collect();
        let temp = real(&std::env::temp_dir());
        if !root.starts_with(&temp) {
            write.push(temp);
        }
        for file in &permissions.files {
            write.push(inside(&root, file)?);
        }
        Ok(Self { root, read: vec![real(dir)], write, network: !permissions.network.is_empty() })
    }

    /// The same policy, letting the plugin read and write its data folder too.
    #[must_use]
    pub fn writing(mut self, data: Option<&Path>) -> Self {
        self.write.extend(data.map(real));
        self
    }

    /// What to run so that `entry` runs confined to this policy: on Linux `launcher` (`vc`) as
    /// the launcher, or `entry` itself without one; on macOS `sandbox-exec` with its profile.
    pub fn entry(&self, launcher: Option<&Path>, entry: Entry) -> Result<Entry, String> {
        platform::entry(self, launcher, entry)
    }
}

/// Runs this process as the plugin launcher when `args` (this process's, the program first) ask it to.
///
/// It confines this process to the policy they carry and becomes the plugin they name, returning
/// only why it could not; `None` when they do not.
pub fn launch(mut args: impl Iterator<Item = OsString>) -> Option<String> {
    let _program = args.next();
    if args.next()? != LAUNCHER_ARG {
        return None;
    }
    let (Some(policy), Some(program)) = (args.next(), args.next()) else {
        return Some("the plugin launcher takes a policy and a program".to_owned());
    };
    let policy = match serde_json::from_str::<Policy>(&policy.to_string_lossy()) {
        Ok(policy) => policy,
        Err(error) => return Some(format!("the plugin launcher's policy is not valid: {error}")),
    };
    Some(platform::launch(&policy, &program, args.collect()))
}

/// The real path of `file`, a path relative to the project at `root` that stays inside it;
/// why not otherwise.
fn inside(root: &Path, file: &str) -> Result<PathBuf, String> {
    let path = Path::new(file);
    let plain =
        path.components().next().is_some() && path.components().all(|part| matches!(part, Component::Normal(_)));
    match beneath(&root.join(path)) {
        Some(found) if plain && found.starts_with(root) => Ok(found),
        _ => Err(format!("its file permission `{file}` is not a path inside the project")),
    }
}

/// The real path of `path`, which need not exist yet: its nearest existing ancestor with links
/// resolved, the rest appended. `None` when that ancestor cannot be resolved (a link to nowhere).
fn beneath(path: &Path) -> Option<PathBuf> {
    let existing = path.ancestors().find(|ancestor| ancestor.symlink_metadata().is_ok())?;
    let real = canonical(existing)?;
    let rest = path.strip_prefix(existing).ok()?;
    Some(if rest.as_os_str().is_empty() { real } else { real.join(rest) })
}

/// `path` with its links resolved, or as given when it does not exist yet.
fn real(path: &Path) -> PathBuf {
    canonical(path).unwrap_or_else(|| path.to_path_buf())
}

/// `path` with its links resolved, written as a path that does not exist is ([`plain`]), so the
/// two compare; `None` when it cannot be resolved.
fn canonical(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok().map(plain)
}

/// A resolved drive path without the `\\?\` that Windows puts before it, which a path that does
/// not exist (the project's, as given) lacks.
#[cfg(windows)]
fn plain(path: PathBuf) -> PathBuf {
    let drive = path.to_str().and_then(|text| text.strip_prefix(r"\\?\"));
    let drive = drive.filter(|rest| rest.as_bytes().get(1) == Some(&b':')).map(PathBuf::from);
    drive.unwrap_or(path)
}

/// `path` itself: only Windows writes a resolved path apart.
#[cfg(not(windows))]
const fn plain(path: PathBuf) -> PathBuf {
    path
}

/// Where the OS cannot confine a plugin: its permissions stay declared only.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod declared {
    use std::ffi::{OsStr, OsString};
    use std::path::Path;

    use super::{Entry, Policy};

    /// A plugin runs unconfined, with a warning.
    pub const FAIL_CLOSED: bool = false;

    /// Why a plugin cannot be confined here.
    pub fn ready(_launcher: Option<&Path>) -> Result<(), String> {
        Err("this OS cannot confine a plugin".to_owned())
    }

    /// `entry` as it is.
    #[expect(
        clippy::unnecessary_wraps,
        clippy::missing_const_for_fn,
        reason = "the same signature as the Linux and macOS `entry`"
    )]
    pub fn entry(_policy: &Policy, _launcher: Option<&Path>, entry: Entry) -> Result<Entry, String> {
        Ok(entry)
    }

    /// None: nothing is confined here.
    pub const fn uncreatable(_policy: &Policy) -> Vec<std::path::PathBuf> {
        Vec::new()
    }

    /// Why there is no launcher here.
    pub fn launch(_policy: &Policy, _program: &OsStr, _args: Vec<OsString>) -> String {
        "the plugin launcher confines plugins on Linux only".to_owned()
    }
}

#[cfg(test)]
#[path = "confine_tests.rs"]
mod tests;
