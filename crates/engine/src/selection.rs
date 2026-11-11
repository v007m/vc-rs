use vc_diagnostics::{Diagnostics, Label};
use vc_model::{SourceFile, SourceFileId, SourceFiles};

use crate::EngineError;

/// The files a check reports on: those at or under one of the requested paths, or every file
/// when none was requested.
pub struct Selection {
    prefixes: Vec<String>,
}

impl Selection {
    /// The selection for `paths`, each relative to the root with `/` separators; `""` or `.`
    /// selects every file.
    ///
    /// # Errors
    ///
    /// [`EngineError::NoFilesMatch`] for the first path that selects no file in `files`.
    pub fn new(paths: &[String], files: &SourceFiles) -> Result<Self, EngineError> {
        let prefixes: Vec<String> = paths.iter().map(|path| normalise(path)).collect();
        for (path, prefix) in paths.iter().zip(&prefixes) {
            if !all(files).any(|file| under(file.path.as_str(), prefix)) {
                return Err(EngineError::NoFilesMatch { path: path.clone() });
            }
        }
        Ok(Self { prefixes })
    }

    /// The diagnostics with a label in a selected file: at their primary span, or at a place
    /// involved, such as the test call that reaches an api's unset var (V2101).
    pub fn keep(&self, diagnostics: Diagnostics, files: &SourceFiles) -> Diagnostics {
        let selected = |label: &Label| files.get(label.span.file).is_some_and(|file| self.selects(file));
        diagnostics
            .into_iter()
            .filter(|d| std::iter::once(&d.primary).chain(&d.secondary).any(selected))
            .collect()
    }

    /// How many files are selected.
    pub fn count(&self, files: &SourceFiles) -> usize {
        all(files).filter(|file| self.selects(file)).count()
    }

    fn selects(&self, file: &SourceFile) -> bool {
        self.prefixes.is_empty() || self.prefixes.iter().any(|prefix| under(file.path.as_str(), prefix))
    }
}

fn all(files: &SourceFiles) -> impl Iterator<Item = &SourceFile> {
    (0..files.len()).filter_map(|index| files.get(SourceFileId::new(u32::try_from(index).ok()?)))
}

/// `path` with `\` as `/`, and no `./` parts or trailing `/`.
fn normalise(path: &str) -> String {
    let slashed = path.replace('\\', "/");
    slashed.split('/').filter(|part| !part.is_empty() && *part != ".").collect::<Vec<_>>().join("/")
}

/// Whether `path` is `prefix` or inside it; everything is under the empty prefix.
fn under(path: &str, prefix: &str) -> bool {
    prefix.is_empty() || path.strip_prefix(prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::{normalise, under};

    #[test]
    fn a_prefix_selects_whole_path_parts() {
        assert!(under("services/users/users.api.yaml", "services/users"));
        assert!(under("services/users/users.api.yaml", "services/users/users.api.yaml"));
        assert!(!under("services/users2/x.api.yaml", "services/users"));
        assert!(under("anything.api.yaml", ""));
    }

    #[test]
    fn paths_are_normalised() {
        assert_eq!(normalise(r".\services\users\"), "services/users");
        assert_eq!(normalise("./"), "");
    }
}
