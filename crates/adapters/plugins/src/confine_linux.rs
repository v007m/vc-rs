//! Linux: the plugin runs under `vc` as its launcher, which confines itself with Landlock and then
//! becomes the plugin, so the plugin's process inherits the rules. The launcher is
//! single-threaded when it confines itself (before `vc` starts a runtime), sets `no_new_privs`,
//! and needs no `unsafe` (D254). Landlock is best effort (D260): on a kernel without it the plugin
//! runs unconfined, with a warning, and on one without its network rules (before 6.7) what the
//! kernel cannot enforce stays declared only.

use std::ffi::{OsStr, OsString};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};

use landlock::{
    ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible as _, Ruleset, RulesetAttr, RulesetCreatedAttr,
    RulesetError, path_beneath_rules,
};

use crate::confine::{Entry, LAUNCHER_ARG, Policy};

/// The Landlock version whose rights are asked for: file rights, and TCP bind and connect.
const LANDLOCK: ABI = ABI::V4;

/// A plugin that cannot be confined runs unconfined, with a warning: Landlock is best effort.
pub const FAIL_CLOSED: bool = false;

/// Whether a plugin can be confined: `vc` is its `launcher` and the kernel has Landlock; why not
/// otherwise.
pub fn ready(launcher: Option<&Path>) -> Result<(), String> {
    if launcher.is_none() {
        return Err("`vc` cannot find its own program to run as the plugin launcher".to_owned());
    }
    let probe = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_all(ABI::V1))
        .and_then(Ruleset::create);
    probe.map(drop).map_err(|error| format!("the kernel has no Landlock ({error})"))
}

/// `launcher` run as the launcher of `entry` under `policy`; `entry` itself without a launcher.
pub fn entry(policy: &Policy, launcher: Option<&Path>, (program, args): Entry) -> Result<Entry, String> {
    let Some(launcher) = launcher else {
        return Ok((program, args));
    };
    let policy = serde_json::to_string(policy).map_err(|error| format!("cannot hand its policy to `vc`: {error}"))?;
    let mut argv = vec![OsString::from(LAUNCHER_ARG), OsString::from(policy), program.into_os_string()];
    argv.extend(args);
    Ok((launcher.to_path_buf(), argv))
}

/// Confines this process to `policy` and becomes `program`; returns only why it could not.
pub fn launch(policy: &Policy, program: &OsStr, args: Vec<OsString>) -> String {
    if let Err(error) = confine(policy) {
        return format!("cannot confine the plugin: {error}");
    }
    let error = std::process::Command::new(program).args(args).exec();
    format!("cannot run `{}`: {error}", program.to_string_lossy())
}

/// Restricts this process, and every process it becomes or starts, to `policy`.
fn confine(policy: &Policy) -> Result<(), RulesetError> {
    let mut ruleset = Ruleset::default().handle_access(AccessFs::from_all(LANDLOCK))?;
    if !policy.network {
        ruleset = ruleset.handle_access(AccessNet::from_all(LANDLOCK))?;
    }
    let readable = outside(&policy.root).into_iter().chain(policy.read.iter().cloned());
    ruleset
        .create()?
        .add_rules(path_beneath_rules(readable, AccessFs::from_read(LANDLOCK)))?
        .add_rules(path_beneath_rules(&policy.write, AccessFs::from_all(LANDLOCK)))?
        .restrict_self()?;
    Ok(())
}

/// Everything outside `root`: at each folder from `root`'s parent up to `/`, every entry but the
/// one on the way to `root`. Landlock only grants, so this is how all but the project is opened.
/// A rule follows links, so an entry that leads back into `root`, or to a folder holding it, is
/// left out.
fn outside(root: &Path) -> Vec<PathBuf> {
    let ancestors = root.ancestors().zip(root.ancestors().skip(1));
    ancestors.flat_map(|(on_the_way, parent)| beside(parent, on_the_way)).filter(|path| apart(path, root)).collect()
}

/// Whether `path`, links resolved, leads neither into `root` (a real path) nor to a folder
/// holding it; a path that cannot be resolved leads nowhere and is left out too.
fn apart(path: &Path, root: &Path) -> bool {
    std::fs::canonicalize(path).is_ok_and(|real| !real.starts_with(root) && !root.starts_with(&real))
}

/// The entries of `parent` other than `on_the_way`; none when it cannot be read.
fn beside(parent: &Path, on_the_way: &Path) -> Vec<PathBuf> {
    let entries = std::fs::read_dir(parent).into_iter().flatten().flatten();
    entries.map(|entry| entry.path()).filter(|path| path != on_the_way).collect()
}

#[cfg(test)]
#[path = "confine_linux_tests.rs"]
mod tests;
