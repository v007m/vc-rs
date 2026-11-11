//! Whether a plugin's entry program is there to start, found as its host finds it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use vc_interfaces::PluginManifest;

/// The endings Windows tries on a program named without one.
const WINDOWS_ENDINGS: [&str; 4] = ["exe", "cmd", "bat", "com"];

/// Whether the program `manifest`'s `entry.command` names can be found for a plugin in `dir`.
///
/// It is looked for as [`crate::StdioPluginHost`] starts it: a file in `dir`, a path to a file,
/// or a program on this process's `PATH`. It does not check the file can be run.
#[must_use]
pub fn entry_found(manifest: &PluginManifest, dir: &Path) -> bool {
    let command = Path::new(&manifest.command);
    if dir.join(command).is_file() {
        return true;
    }
    if command.is_absolute() || command.components().count() > 1 {
        return command.is_file();
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|folder| named(command).iter().any(|name| folder.join(name).is_file()))
}

/// The file names a program called `command` can have: on Windows, also with each of
/// [`WINDOWS_ENDINGS`] when it has none.
fn named(command: &Path) -> Vec<PathBuf> {
    let mut names = vec![command.to_path_buf()];
    if cfg!(windows) && command.extension().is_none() {
        names.extend(WINDOWS_ENDINGS.iter().map(|ending| {
            let mut name = OsString::from(command.as_os_str());
            name.push(".");
            name.push(ending);
            PathBuf::from(name)
        }));
    }
    names
}

#[cfg(test)]
mod tests {
    use std::fs;

    use vc_interfaces::{PluginKind, PluginManifest, PluginPermissions};

    use super::entry_found;

    fn manifest(command: &str) -> PluginManifest {
        PluginManifest {
            id: "report".into(),
            version: "1.0.0".into(),
            kind: PluginKind::Reporter,
            command: command.into(),
            args: Vec::new(),
            permissions: PluginPermissions::default(),
        }
    }

    #[test]
    fn an_entry_is_a_file_in_the_plugin_s_folder_a_path_or_a_program_on_path() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("report.sh"), "#!/bin/sh\n").unwrap();
        assert!(entry_found(&manifest("report.sh"), dir.path()), "a file in the folder");
        let elsewhere = dir.path().join("report.sh").to_string_lossy().into_owned();
        assert!(entry_found(&manifest(&elsewhere), &dir.path().join("other")), "a path to a file");
        let on_path = if cfg!(windows) { "cmd" } else { "sh" };
        assert!(entry_found(&manifest(on_path), dir.path()), "a program on PATH");
        assert!(!entry_found(&manifest("vibranium-no-such-program"), dir.path()));
        assert!(!entry_found(&manifest("bin/missing.sh"), dir.path()));
    }
}
