use std::path::{Path, PathBuf};

use ignore::{DirEntry, WalkBuilder};
use vc_interfaces::{Discovery, SourceError, SourceFilter, SourceProvider, SourceWriter};
use vc_model::SourcePath;

/// Reads a project's definition files from disk, and lists the other files as assets; reads
/// any file by path on request.
///
/// Walks the root with `ignore`, so `.gitignore` (and `.ignore`) files are respected even
/// outside a git repository; the user's global git excludes are not, so every machine finds
/// the same files. Folders the filter excludes are not entered. Links to folders are not
/// followed; a link to a file is read only when its target is inside the root.
#[derive(Clone, Debug)]
pub struct FsSources {
    root: PathBuf,
}

impl FsSources {
    /// A provider for the project at `root`.
    ///
    /// # Errors
    ///
    /// [`SourceError::Root`] when `root` cannot be resolved.
    pub fn new(root: &Path) -> Result<Self, SourceError> {
        let root = std::fs::canonicalize(root).map_err(|error| root_error(root, &error))?;
        Ok(Self { root })
    }

    /// The project root, with links resolved.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn walk(&self, filter: &SourceFilter) -> ignore::Walk {
        let (root, dirs) = (self.root.clone(), filter.clone());
        let mut walk = WalkBuilder::new(&self.root);
        walk.hidden(false).git_global(false).require_git(false).follow_links(false);
        walk.filter_entry(move |entry| relative(&root, entry.path()).is_none_or(|(text, _)| !dirs.skips(&text)));
        walk.build()
    }

    fn visit(&self, entry: &DirEntry, filter: &SourceFilter, found: &mut Discovery) {
        let Some(kind) = entry.file_type() else { return };
        if kind.is_dir() {
            return;
        }
        let Some((text, utf8)) = relative(&self.root, entry.path()) else { return };
        match filter.select(&text) {
            Some(Ok(path)) if utf8 => match self.load(path, entry.path()) {
                Ok(file) => found.files.push(file),
                Err(problem) => found.problems.push(problem),
            },
            Some(Ok(path)) => found.problems.push(SourceError::InvalidPath { path: path.to_string() }),
            Some(Err(problem)) => found.problems.push(problem),
            None if utf8 => found.assets.extend(filter.asset(&text)),
            None => {}
        }
    }

    /// Reads the file through its resolved path, so a link cannot lead out of the root.
    fn load(&self, path: SourcePath, on_disk: &Path) -> Result<vc_model::SourceFile, SourceError> {
        let failed = |error: std::io::Error| SourceError::Read { path: path.to_string(), reason: error.to_string() };
        let target = std::fs::canonicalize(on_disk).map_err(failed)?;
        if !target.starts_with(&self.root) {
            return Err(SourceError::OutsideRoot { path: path.to_string() });
        }
        let bytes = std::fs::read(&target).map_err(failed)?;
        vc_interfaces::source_file(path, bytes)
    }
}

impl SourceProvider for FsSources {
    fn discover(&self, filter: &SourceFilter) -> Result<Discovery, SourceError> {
        if let Err(error) = std::fs::read_dir(&self.root) {
            return Err(root_error(&self.root, &error));
        }
        let mut found = Discovery::default();
        for entry in self.walk(filter) {
            match entry {
                Ok(entry) => self.visit(&entry, filter, &mut found),
                Err(error) => found.problems.push(self.walk_problem(&error)),
            }
        }
        found.sort();
        Ok(found)
    }

    fn read(&self, paths: &[SourcePath]) -> Result<Discovery, SourceError> {
        if let Err(error) = std::fs::read_dir(&self.root) {
            return Err(root_error(&self.root, &error));
        }
        let mut found = Discovery::default();
        for path in unique(paths) {
            match self.load(path.clone(), &self.root.join(path.as_str())) {
                Ok(file) => found.files.push(file),
                Err(problem) => found.problems.push(problem),
            }
        }
        found.sort();
        Ok(found)
    }
}

impl SourceWriter for FsSources {
    /// Writes only inside the root: a folder or file that is a link out of it is refused before
    /// anything is created.
    fn write(&self, path: &SourcePath, text: &str) -> Result<(), SourceError> {
        let failed = |error: std::io::Error| SourceError::Write { path: path.to_string(), reason: error.to_string() };
        let target = self.root.join(path.as_str());
        let existing = target.ancestors().find(|at| at.symlink_metadata().is_ok()).unwrap_or(&self.root);
        if !std::fs::canonicalize(existing).map_err(failed)?.starts_with(&self.root) {
            return Err(SourceError::OutsideRoot { path: path.to_string() });
        }
        if let Some(folder) = target.parent() {
            std::fs::create_dir_all(folder).map_err(failed)?;
        }
        std::fs::write(&target, text).map_err(failed)
    }
}

impl FsSources {
    fn walk_problem(&self, error: &ignore::Error) -> SourceError {
        let path = error_path(error).and_then(|path| relative(&self.root, path)).map(|(text, _)| text);
        SourceError::Read { path: path.unwrap_or_else(|| ".".to_owned()), reason: error.to_string() }
    }
}

/// `path` relative to `root` with `/` separators, and whether every part was UTF-8 (parts that
/// are not are decoded lossily); `None` for the root itself or a path outside it.
fn relative(root: &Path, path: &Path) -> Option<(String, bool)> {
    let rest = path.strip_prefix(root).ok()?;
    let parts: Vec<_> = rest.components().map(std::path::Component::as_os_str).collect();
    if parts.is_empty() {
        return None;
    }
    let utf8 = parts.iter().all(|part| part.to_str().is_some());
    Some((parts.iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>().join("/"), utf8))
}

/// `paths` in order, each once.
fn unique(paths: &[SourcePath]) -> std::collections::BTreeSet<&SourcePath> {
    paths.iter().collect()
}

fn error_path(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => error_path(err),
        _ => None,
    }
}

fn root_error(root: &Path, error: &std::io::Error) -> SourceError {
    SourceError::Root { root: root.display().to_string(), reason: error.to_string() }
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
