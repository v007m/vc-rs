//! A `path:` source: the folder's files copied into the staging folder, hashed as they go, and
//! the same hash over an installed folder, to check it against the lock.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Folders never copied or hashed: version control, the project's own state, and the bytecode
/// Python writes beside the modules a plugin imports.
const SKIPPED: [&str; 3] = [".git", ".vibranium", "__pycache__"];

/// Copies the files under `from` into `to`, each with its permissions (so a script keeps its
/// executable bit), and returns their integrity, `blake3-<hex>` over each file's relative path
/// and bytes in path order. Links are not followed.
pub fn copy(from: &Path, to: &Path) -> Result<String, String> {
    digest(from, |relative, bytes, permissions| write(&to.join(relative), bytes, permissions))
}

/// The integrity of the files under `root`, as [`copy`] returns it.
pub fn integrity(root: &Path) -> Result<String, String> {
    digest(root, |_, _, _| Ok(()))
}

/// Hands each file under `from` to `each`, with its bytes and permissions, and returns their hash.
fn digest(
    from: &Path,
    mut each: impl FnMut(&Path, &[u8], fs::Permissions) -> io::Result<()>,
) -> Result<String, String> {
    let files = files(from).map_err(|error| format!("{}: {error}", from.display()))?;
    let mut hasher = blake3::Hasher::new();
    for relative in &files {
        let source = from.join(relative);
        let named = |error: io::Error| format!("{}: {error}", relative.display());
        let bytes = fs::read(&source).map_err(named)?;
        each(relative, &bytes, fs::metadata(&source).map_err(named)?.permissions()).map_err(named)?;
        hash(&mut hasher, relative, &bytes);
    }
    Ok(format!("blake3-{}", hasher.finalize().to_hex()))
}

/// The files under `root`, relative to it and sorted.
fn files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut folders = vec![PathBuf::new()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(root.join(&folder))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let relative = folder.join(entry.file_name());
            if kind.is_dir() && !SKIPPED.iter().any(|skipped| entry.file_name() == *skipped) {
                folders.push(relative);
            } else if kind.is_file() {
                found.push(relative);
            }
        }
    }
    found.sort();
    Ok(found)
}

fn write(target: &Path, bytes: &[u8], permissions: fs::Permissions) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target, bytes)?;
    fs::set_permissions(target, permissions)
}

/// One file into the hash: its path with `/` separators, a zero byte, its length and its bytes.
fn hash(hasher: &mut blake3::Hasher, relative: &Path, bytes: &[u8]) {
    let parts = relative.components().map(|part| part.as_os_str().to_string_lossy()).collect::<Vec<_>>();
    hasher.update(parts.join("/").as_bytes());
    hasher.update(&[0]);
    hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{copy, integrity};

    #[test]
    fn copies_the_files_but_not_git_and_hashes_them_the_same_each_time() {
        let from = tempfile::tempdir().unwrap();
        fs::create_dir_all(from.path().join("lib/.git")).unwrap();
        fs::create_dir_all(from.path().join(".git")).unwrap();
        fs::write(from.path().join("vibranium-plugin.yaml"), "id: report\n").unwrap();
        fs::write(from.path().join("lib/main.py"), "print()\n").unwrap();
        fs::write(from.path().join(".git/HEAD"), "ref\n").unwrap();
        let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let integrity = copy(from.path(), first.path()).unwrap();
        assert_eq!(copy(from.path(), second.path()).unwrap(), integrity);
        assert!(integrity.starts_with("blake3-") && integrity.len() == 7 + 64, "{integrity}");
        assert_eq!(fs::read_to_string(first.path().join("lib/main.py")).unwrap(), "print()\n");
        assert!(!first.path().join(".git").exists());
        fs::write(from.path().join("lib/main.py"), "print(1)\n").unwrap();
        assert_ne!(copy(from.path(), tempfile::tempdir().unwrap().path()).unwrap(), integrity, "a change shows");
    }

    #[test]
    fn the_integrity_of_a_copy_is_the_one_copy_returned_and_ignores_python_bytecode() {
        let from = tempfile::tempdir().unwrap();
        fs::write(from.path().join("main.py"), "print()\n").unwrap();
        let to = tempfile::tempdir().unwrap();
        let copied = copy(from.path(), to.path()).unwrap();
        fs::create_dir_all(to.path().join("__pycache__")).unwrap();
        fs::write(to.path().join("__pycache__/lib.pyc"), "x").unwrap();
        assert_eq!(integrity(to.path()).unwrap(), copied);
        fs::write(to.path().join("main.py"), "print(2)\n").unwrap();
        assert_ne!(integrity(to.path()).unwrap(), copied);
    }

    #[cfg(unix)]
    #[test]
    fn a_copied_script_keeps_its_executable_bit() {
        use std::os::unix::fs::PermissionsExt;
        let from = tempfile::tempdir().unwrap();
        let script = from.path().join("bin/run.sh");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let to = tempfile::tempdir().unwrap();
        copy(from.path(), to.path()).unwrap();
        let mode = fs::metadata(to.path().join("bin/run.sh")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn a_missing_folder_is_named() {
        let to = tempfile::tempdir().unwrap();
        let error = copy(&to.path().join("nowhere"), to.path()).unwrap_err();
        assert!(error.contains("nowhere"), "{error}");
    }
}
