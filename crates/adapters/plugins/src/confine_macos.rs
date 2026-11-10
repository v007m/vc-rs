//! macOS: the plugin runs under `sandbox-exec` with a Seatbelt profile written from its policy.
//! In a profile a later rule wins over an earlier one, so each opening follows the closing it
//! narrows. Reading the project is closed for file contents and listings (`file-read-data`), not
//! for metadata, so a runtime can still walk the path to its own folder.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::confine::{Entry, Policy};

/// The program that runs another under a Seatbelt profile.
pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// `sandbox-exec` running `entry` under `policy`'s profile; the launcher is not needed here.
pub fn entry(policy: &Policy, _launcher: Option<&Path>, (program, args): Entry) -> Result<Entry, String> {
    let mut sandboxed = vec![OsString::from("-p"), OsString::from(profile(policy)?), program.into_os_string()];
    sandboxed.extend(args);
    Ok((PathBuf::from(SANDBOX_EXEC), sandboxed))
}

/// Why there is no launcher here.
pub fn launch(_policy: &Policy, _program: OsString, _args: Vec<OsString>) -> String {
    "the plugin launcher confines plugins on Linux only".to_owned()
}

/// The Seatbelt profile of `policy`; why not when one of its paths is not UTF-8.
pub fn profile(policy: &Policy) -> Result<String, String> {
    let written = subpaths(&policy.write)?;
    let mut opened = subpaths(&policy.read)?;
    opened.push(' ');
    opened.push_str(&written);
    let mut rules = vec![
        "(version 1)".to_owned(),
        "(allow default)".to_owned(),
        "(deny file-write*)".to_owned(),
        format!("(allow file-write* {written})"),
        format!("(deny file-read-data (subpath {}))", quoted(&policy.root)?),
        format!("(allow file-read-data {opened})"),
    ];
    if !policy.network {
        rules.push("(deny network*)".to_owned());
    }
    Ok(rules.join("\n"))
}

/// `(subpath "…")` for each of `paths`, space-separated.
fn subpaths(paths: &[PathBuf]) -> Result<String, String> {
    let each: Result<Vec<String>, String> =
        paths.iter().map(|path| Ok(format!("(subpath {})", quoted(path)?))).collect();
    Ok(each?.join(" "))
}

/// `path` as a profile string, `\` and `"` escaped.
fn quoted(path: &Path) -> Result<String, String> {
    let text = path.to_str().ok_or_else(|| format!("its path {} is not UTF-8", path.display()))?;
    Ok(format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"")))
}

#[cfg(test)]
#[path = "confine_macos_tests.rs"]
mod tests;
