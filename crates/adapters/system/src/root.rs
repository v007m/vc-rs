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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{CONFIG_FILE, find_project_root};

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
}
