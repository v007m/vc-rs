use std::path::{Path, PathBuf};

use ignore::{DirEntry, WalkBuilder};
use vc_interfaces::{Discovery, SourceError, SourceFilter, SourceProvider};
use vc_model::{SourcePath, hidden_part};

/// Reads a project's definition files from disk, and lists the other files as assets; reads
/// any file by path on request.
///
/// Walks the root with `ignore`, so `.gitignore` (and `.ignore`) files are respected even
/// outside a git repository; the user's global git excludes are not, so every machine finds
/// the same files. Folders the filter excludes are not entered. Links to folders are not
/// followed; a link to a file is read only when its target is inside the root and outside
/// every hidden file and folder, or, when the link's own path is hidden, a secrets schema
/// outside `.git` and `.vibranium`.
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

    /// The walk discovery makes from the root, before any filter is added.
    fn builder(&self) -> WalkBuilder {
        let mut walk = WalkBuilder::new(&self.root);
        walk.hidden(false).git_global(false).require_git(false).follow_links(false);
        walk
    }

    fn walk(&self, filter: &SourceFilter) -> ignore::Walk {
        let (root, dirs) = (self.root.clone(), filter.clone());
        let mut walk = self.builder();
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
        let target = self.canonical(&path, on_disk)?;
        read_file(path, &target)
    }

    /// Where `on_disk` resolves to, links followed, while that stays in the root.
    fn canonical(&self, path: &SourcePath, on_disk: &Path) -> Result<PathBuf, SourceError> {
        let target = std::fs::canonicalize(on_disk).map_err(|error| read_error(path, &error))?;
        if !self.leads_in(path, &target) {
            return Err(SourceError::OutsideRoot { path: path.to_string() });
        }
        Ok(target)
    }

    /// Reads the data file at `path`, outside the root, through its resolved path, only while
    /// that stays under `allowed` (a `datasets.allowRoots` folder, resolved too) and in no
    /// hidden folder below it. It never lands inside the project: a file there is read by its own
    /// path, under discovery's rules for hidden files and links.
    fn load_outside(&self, path: &SourcePath, allowed: &SourcePath) -> Result<vc_model::SourceFile, SourceError> {
        let outside = || SourceError::OutsideRoot { path: path.to_string() };
        let target = std::fs::canonicalize(self.root.join(path.as_str())).map_err(|error| read_error(path, &error))?;
        let folder = std::fs::canonicalize(self.root.join(allowed.as_str())).map_err(|_| outside())?;
        let rest = target.strip_prefix(&folder).map_err(|_| outside())?;
        if target.starts_with(&self.root)
            || rest.components().any(|part| hidden_part(&part.as_os_str().to_string_lossy()))
        {
            return Err(outside());
        }
        read_file(path.clone(), &target)
    }

    /// Each of `wanted`, asked for once, read by `load`: the files and the problems; an error
    /// when the root cannot be read.
    fn gathered<T: Ord>(
        &self,
        wanted: &[T],
        load: impl Fn(&T) -> Result<vc_model::SourceFile, SourceError>,
    ) -> Result<Discovery, SourceError> {
        if let Err(error) = std::fs::read_dir(&self.root) {
            return Err(root_error(&self.root, &error));
        }
        let mut found = Discovery::default();
        for item in unique(wanted) {
            match load(item) {
                Ok(file) => found.files.push(file),
                Err(problem) => found.problems.push(problem),
            }
        }
        found.sort();
        Ok(found)
    }
}

/// The file at `target`, where `path` resolved to, read and hashed. A link swapped in on the
/// way after `target` was resolved and checked is caught by resolving it again after the read:
/// a file that no longer resolves to `target` is refused. Residual risk: a link swapped in and
/// back again while the file is read goes unseen; std has no safe, portable way to ask an open
/// file for its path, and the crate forbids `unsafe`.
fn read_file(path: SourcePath, target: &Path) -> Result<vc_model::SourceFile, SourceError> {
    let bytes = read_checked(&path, target)?;
    vc_interfaces::source_file(path, bytes)
}

/// The bytes of the file at `target`, where `path` resolved to, refused when it is a folder or
/// no longer resolves to `target` after the read (see [`read_file`]).
fn read_checked(path: &SourcePath, target: &Path) -> Result<Vec<u8>, SourceError> {
    if target.is_dir() {
        return Err(SourceError::Read { path: path.to_string(), reason: "it is a folder, not a file".to_owned() });
    }
    let bytes = std::fs::read(target).map_err(|error| read_error(path, &error))?;
    if std::fs::canonicalize(target).ok().as_deref() != Some(target) {
        return Err(SourceError::OutsideRoot { path: path.to_string() });
    }
    Ok(bytes)
}

/// Why `path` could not be read: missing, or what the system said.
fn read_error(path: &SourcePath, error: &std::io::Error) -> SourceError {
    match error.kind() {
        std::io::ErrorKind::NotFound => SourceError::Missing { path: path.to_string() },
        _ => SourceError::Read { path: path.to_string(), reason: error.to_string() },
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
        self.gathered(paths, |path| self.load(path.clone(), &self.root.join(path.as_str())))
    }

    fn read_outside(&self, wanted: &[(SourcePath, SourcePath)]) -> Result<Discovery, SourceError> {
        self.gathered(wanted, |(path, allowed)| self.load_outside(path, allowed))
    }

    fn read_bytes(&self, path: &SourcePath) -> Result<Vec<u8>, SourceError> {
        let target = self.canonical(path, &self.root.join(path.as_str()))?;
        read_checked(path, &target)
    }

    /// Walks only the folders on the way to `path`, with the same ignore rules as
    /// [`SourceProvider::discover`], so a file in an ignored folder is never reached. Discovery
    /// does not follow a link to a folder, so a path under one, there or not yet, is left out
    /// too. A file or folder not there yet is matched against the ignore files in the folders
    /// above it.
    fn ignores(&self, path: &SourcePath) -> bool {
        let target = self.root.join(path.as_str());
        let Some(nearest) = target.ancestors().find(|at| at.symlink_metadata().is_ok()) else { return false };
        self.under_link(&target) || !self.reached(nearest) || (nearest != target && name_ignored(&target))
    }

    fn real(&self, path: &SourcePath) -> Option<SourcePath> {
        let real = crate::project_path(&self.root, &self.root.join(path.as_str()))?;
        SourcePath::new(&real).ok()
    }
}

impl FsSources {
    /// Whether `resolved`, where `path` leads on disk, is inside the root and outside every
    /// hidden file or folder. A hidden `path` may lead to itself, or to a secrets schema in a
    /// hidden folder ([`schema_target`]); a link named `x.test.yaml` cannot open `.env`, and a
    /// link named `.env.schema` may open `.config/env.schema` but not `.env`, `.npmrc` or
    /// `.git/config` (D247).
    fn leads_in(&self, path: &SourcePath, resolved: &Path) -> bool {
        let Ok(rest) = resolved.strip_prefix(&self.root) else { return false };
        let hidden = rest.components().any(|part| hidden_part(&part.as_os_str().to_string_lossy()));
        !hidden || (path.is_hidden() && (rest == Path::new(path.as_str()) || schema_target(rest)))
    }

    /// Whether discovery's walk reaches `at`, which is there: no ignore file leaves it or a
    /// folder above it out, and no link to a folder leads to it.
    /// Whether a folder between the root and `target` is a link, which discovery never walks
    /// into.
    fn under_link(&self, target: &Path) -> bool {
        let folders = target.ancestors().skip(1).take_while(|at| *at != self.root);
        folders.filter_map(|at| at.symlink_metadata().ok()).any(|found| found.file_type().is_symlink())
    }

    fn reached(&self, at: &Path) -> bool {
        if at == self.root {
            return true;
        }
        let on_the_way: Vec<PathBuf> = at.ancestors().map(Path::to_path_buf).collect();
        let mut walk = self.builder();
        walk.filter_entry(move |entry| on_the_way.iter().any(|folder| folder == entry.path()));
        walk.build().flatten().any(|entry| entry.path() == at)
    }

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

/// Whether an ignore file in a folder above `target`, which is not there, leaves it or a folder
/// it would be in out: the nearest file that matches decides, `.ignore` before `.gitignore`, as
/// the walk reads them.
fn name_ignored(target: &Path) -> bool {
    let files = target.ancestors().skip(1).flat_map(|folder| [folder.join(".ignore"), folder.join(".gitignore")]);
    let decide = |file: PathBuf| {
        let (rules, _) = ignore::gitignore::Gitignore::new(file);
        let matched = rules.matched_path_or_any_parents(target, false);
        (!matched.is_none()).then(|| matched.is_ignore())
    };
    files.filter(|file| file.is_file()).find_map(decide).unwrap_or(false)
}

/// Whether `rest` is a hidden file a hidden link may lead to: one named `.env.schema` or
/// `env.schema`, in any case, outside `.git` and `.vibranium`. Every other hidden file is
/// refused, so a cloned repo's link cannot open git's config or the token cache (D247).
fn schema_target(rest: &Path) -> bool {
    let lower = |part: &std::ffi::OsStr| part.to_string_lossy().to_ascii_lowercase();
    let Some(name) = rest.file_name().map(lower) else { return false };
    let guarded = rest.components().any(|part| matches!(lower(part.as_os_str()).as_str(), ".git" | ".vibranium"));
    (name == ".env.schema" || name == "env.schema") && !guarded
}


/// `paths` in order, each once.
fn unique<T: Ord>(items: &[T]) -> std::collections::BTreeSet<&T> {
    items.iter().collect()
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

#[path = "sources_write.rs"]
mod write;

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
