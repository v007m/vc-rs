//! `vc fmt`: every definition file at or under the paths given, rewritten in the house style
//! (D180): block YAML with a 2-space indent and keys in the order of the shape definitions. It
//! keeps comments, and with `strip_comments` drops the one-line comments `vc new` wrote.

use vc_interfaces::source_file;
use vc_model::{FileKind, SourceFileId, SourcePath};
use vc_parser::FormatRules;

use crate::authoring::{Authoring, refused};
use crate::create::DOCS;
use crate::edit::{Changes, FileChange, internal};
use crate::{Engine, EngineError};

/// The house style of one kind of file.
struct HouseStyle {
    kind: FileKind,
    strip: bool,
}

impl FormatRules for HouseStyle {
    fn key_order(&self, path: &[&str]) -> Option<Vec<&'static str>> {
        vc_compiler::key_order(self.kind, path)
    }

    fn drops(&self, comment: &str) -> bool {
        self.strip && is_generated(comment)
    }
}

impl Engine {
    /// The change that formats the definition files at or under `paths` (relative to the
    /// project root, `/` separators; none means all), dropping generated comments when
    /// `strip_comments` is set. Files already formatted are left out.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when a file does not parse or cannot be formatted without changing
    /// what it says, or when the guard refuses the change.
    pub fn fmt(&self, paths: &[String], strip_comments: bool) -> Result<Changes, EngineError> {
        let project = self.authoring()?;
        let mut changes = Changes::default();
        let wanted = project
            .entries()
            .filter(|(_, file)| paths.is_empty() || paths.iter().any(|path| under(file.path.as_str(), path)));
        for (id, file) in wanted {
            let Some(kind) = file.kind() else { continue };
            let opened = Authoring::open((id, file))?;
            let after = opened.document.format(&HouseStyle { kind, strip: strip_comments }).map_err(|error| {
                refused(
                    format!("{}: {error}", file.path),
                    "format it by hand, or move the comment out of the `[..]` or `{..}`",
                )
            })?;
            if after != file.text.as_str() {
                changes.titles.push(format!("Format {}", file.path));
                changes.files.push(FileChange {
                    path: file.path.to_string(),
                    before: Some(file.text.clone()),
                    after,
                    moved_from: None,
                });
            }
        }
        if changes.is_empty() { Ok(changes) } else { self.guarded(&project, changes) }
    }
}

/// `text`, a new file at `path` of kind `kind`, in the house style.
pub fn house_style(path: SourcePath, kind: FileKind, text: &str) -> Result<String, EngineError> {
    let file = source_file(path, text.as_bytes().to_vec()).map_err(internal)?;
    let opened = Authoring::open((SourceFileId::new(0), &file))?;
    opened.document.format(&HouseStyle { kind, strip: false }).map_err(internal)
}

/// Whether `file` is `wanted` or inside the folder `wanted`; an empty `wanted` is the root.
fn under(file: &str, wanted: &str) -> bool {
    let wanted = wanted.trim_end_matches('/');
    wanted.is_empty() || file == wanted || file.strip_prefix(wanted).is_some_and(|rest| rest.starts_with('/'))
}

/// Whether `comment` is one `vc new` wrote above an item: `# users#get · GET /users · keys: …`.
pub fn is_generated(comment: &str) -> bool {
    comment.starts_with('#') && comment.contains(&format!(" · keys: {DOCS}#"))
}

#[cfg(test)]
mod tests {
    use super::{DOCS, is_generated, under};

    #[test]
    fn a_path_covers_itself_and_what_is_under_it() {
        assert!(under("services/users/crud.test.yaml", "services/users"));
        assert!(under("services/users/crud.test.yaml", "services/users/"));
        assert!(under("services/users/crud.test.yaml", "services/users/crud.test.yaml"));
        assert!(under("services/users/crud.test.yaml", ""));
        assert!(!under("services/users2/crud.test.yaml", "services/users"));
    }

    #[test]
    fn only_the_comments_vc_writes_are_generated() {
        assert!(is_generated(&format!("# users#get · GET /users · keys: {DOCS}#api-files")));
        assert!(!is_generated("# the users service"));
        assert!(!is_generated("# a · b"));
    }
}
