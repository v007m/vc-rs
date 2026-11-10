//! The project on disk each acceptance test runs `vc` in, shared by the test binaries that
//! include this file with `#[path]`.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

/// A project on disk and a home folder of its own, so no user config leaks in.
pub struct Project {
    root: TempDir,
    home: TempDir,
}

impl Project {
    /// A project holding `files`.
    pub fn new(files: &[(&str, &str)]) -> Self {
        let project = Self { root: tempfile::tempdir().unwrap(), home: tempfile::tempdir().unwrap() };
        for (path, text) in files {
            project.write(path, text);
        }
        project
    }

    pub fn path(&self) -> &Path {
        self.root.path()
    }

    pub fn home(&self) -> &Path {
        self.home.path()
    }

    /// Writes `text` to `path` under the project root.
    pub fn write(&self, path: &str, text: &str) {
        let file = self.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
}
