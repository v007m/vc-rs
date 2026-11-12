use std::path::{Path, PathBuf};

/// The file that marks a project's root folder.
pub const CONFIG_FILE: &str = "vibranium.config.yaml";

/// The nearest folder at or above `start` that holds [`CONFIG_FILE`], with links resolved.
///
/// `None` when there is none or `start` does not exist. Every path a run prints starts with it,
/// so it is written as the user writes it: `C:\x` on Windows, not `\\?\C:\x`.
#[must_use]
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let start = plain(std::fs::canonicalize(start).ok()?);
    start.ancestors().find(|dir| dir.join(CONFIG_FILE).is_file()).map(Path::to_path_buf)
}

/// A resolved drive path without the `\\?\` Windows writes before it (`\\?\C:\x` is `C:\x`);
/// any other path as it is. Pure, so every OS tests it.
fn plain(path: PathBuf) -> PathBuf {
    let drive = path.to_str().and_then(|text| text.strip_prefix(r"\\?\"));
    let drive = drive.filter(|rest| rest.as_bytes().get(1) == Some(&b':')).map(PathBuf::from);
    drive.unwrap_or(path)
}

/// `path` relative to `root`, links resolved in both, with `/` between its parts; `None` when it
/// does not exist or is not under `root`.
#[must_use]
pub fn project_path(root: &Path, path: &Path) -> Option<String> {
    let resolved = std::fs::canonicalize(path).ok()?;
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let relative = resolved.strip_prefix(&root).ok()?;
    let parts: Vec<_> = relative.components().map(|part| part.as_os_str().to_string_lossy()).collect();
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::{CONFIG_FILE, find_project_root, plain, project_path};

    #[test]
    fn finds_the_nearest_folder_with_a_config() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir_all(root.join("services/users")).unwrap();
        fs::write(root.join(CONFIG_FILE), "syntaxVersion: 2\n").unwrap();
        let found = find_project_root(&root.join("services/users")).unwrap();
        assert_eq!(found, plain(fs::canonicalize(&root).unwrap()));
        assert_eq!(find_project_root(&root).unwrap(), found);
        assert!(!found.to_string_lossy().starts_with(r"\\?\"), "{}", found.display());
    }

    #[test]
    fn only_a_drive_path_loses_its_verbatim_prefix() {
        assert_eq!(plain(PathBuf::from(r"\\?\C:\x")), PathBuf::from(r"C:\x"));
        for kept in [r"\\?\UNC\host\share", r"\\?\Volume{1}\x", "/tmp/x", r"C:\x", ""] {
            assert_eq!(plain(PathBuf::from(kept)), PathBuf::from(kept), "{kept}");
        }
    }

    #[test]
    fn finds_nothing_without_a_config() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find_project_root(dir.path()), None);
        assert_eq!(find_project_root(&dir.path().join("missing")), None);
    }

    #[test]
    fn a_path_in_the_project_is_made_relative_to_its_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        fs::write(root.join("specs/shop.yaml"), "openapi: 3.1.0\n").unwrap();
        assert_eq!(project_path(&root, &root.join("specs/../specs/shop.yaml")).as_deref(), Some("specs/shop.yaml"));
        assert_eq!(project_path(&root, &root.join("specs/missing.yaml")), None);
        assert_eq!(project_path(&root.join("specs"), &root), None, "the root is outside a folder in it");
    }

    #[cfg(unix)]
    #[test]
    fn a_root_given_through_a_link_or_dot_dot_still_holds_its_files() {
        let dir = tempfile::tempdir().unwrap();
        let (real, link) = (dir.path().join("real"), dir.path().join("link"));
        fs::create_dir_all(real.join("specs")).unwrap();
        fs::write(real.join("specs/shop.yaml"), "openapi: 3.1.0\n").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(project_path(&link, &link.join("specs/shop.yaml")).as_deref(), Some("specs/shop.yaml"));
        assert_eq!(project_path(&link, &real.join("specs/shop.yaml")).as_deref(), Some("specs/shop.yaml"));
        let dotted = real.join("specs/..");
        assert_eq!(project_path(&dotted, &real.join("specs/shop.yaml")).as_deref(), Some("specs/shop.yaml"));
    }
}
