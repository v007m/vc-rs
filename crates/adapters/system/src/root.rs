use std::path::{Path, PathBuf};

/// The file that marks a project's root folder.
pub const CONFIG_FILE: &str = "vibranium.config.yaml";

/// The nearest folder at or above `start` that holds [`CONFIG_FILE`], with links resolved;
/// `None` when there is none or `start` does not exist.
#[must_use]
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let start = std::fs::canonicalize(start).ok()?;
    start.ancestors().find(|dir| dir.join(CONFIG_FILE).is_file()).map(Path::to_path_buf)
}

/// `path` relative to `root`, links resolved, with `/` between its parts; `None` when it does
/// not exist or is not under `root`.
#[must_use]
pub fn project_path(root: &Path, path: &Path) -> Option<String> {
    let resolved = std::fs::canonicalize(path).ok()?;
    let relative = resolved.strip_prefix(root).ok()?;
    let parts: Vec<_> = relative.components().map(|part| part.as_os_str().to_string_lossy()).collect();
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{CONFIG_FILE, find_project_root, project_path};

    #[test]
    fn finds_the_nearest_folder_with_a_config() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir_all(root.join("services/users")).unwrap();
        fs::write(root.join(CONFIG_FILE), "syntaxVersion: 2\n").unwrap();
        let found = find_project_root(&root.join("services/users")).unwrap();
        assert_eq!(found, fs::canonicalize(&root).unwrap());
        assert_eq!(find_project_root(&root).unwrap(), found);
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
}
