//! A `path:` source: the folder's files copied into the staging folder, hashed as they go.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Folders never copied: version control and the project's own state.
const SKIPPED: [&str; 2] = [".git", ".vibranium"];

/// Copies the files under `from` into `to` and returns their integrity, `blake3-<hex>` over each
/// file's relative path and bytes in path order. Links are not followed.
pub fn copy(from: &Path, to: &Path) -> Result<String, String> {
    let files = files(from).map_err(|error| format!("{}: {error}", from.display()))?;
    let mut hasher = blake3::Hasher::new();
    for relative in &files {
        let bytes = fs::read(from.join(relative)).map_err(|error| format!("{}: {error}", relative.display()))?;
        write(&to.join(relative), &bytes).map_err(|error| format!("{}: {error}", relative.display()))?;
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

fn write(target: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target, bytes)
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

    use super::copy;

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
    fn a_missing_folder_is_named() {
        let to = tempfile::tempdir().unwrap();
        let error = copy(&to.path().join("nowhere"), to.path()).unwrap_err();
        assert!(error.contains("nowhere"), "{error}");
    }
}
