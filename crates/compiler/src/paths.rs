//! The config's `paths.include` and `paths.exclude`, which decide what discovery reads.

use vc_model::{FileKind, SourceFile, SourceFileId};
use vc_parser::{Node, Syntax};

/// The discovery globs the project's config writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceGlobs {
    /// `paths.include`; empty to read every definition file.
    pub include: Vec<String>,
    /// `paths.exclude`, or `None` to keep the default excludes.
    pub exclude: Option<Vec<String>>,
}

impl SourceGlobs {
    /// The globs the root config among `files` writes. The defaults when there is no config,
    /// it does not parse (its diagnostics come from the compile), or it writes no `paths`.
    #[must_use]
    pub fn of(files: &[SourceFile]) -> Self {
        let config = files.iter().find(|file| file.kind() == Some(FileKind::Config));
        config.and_then(read).unwrap_or_default()
    }

    /// Whether these are the globs discovery uses when the config writes none.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn read(config: &SourceFile) -> Option<SourceGlobs> {
    let (node, _) = vc_parser::parse(SourceFileId::new(0), Syntax::of(&config.path), &config.text);
    let node = node?;
    let paths = node.get("paths")?;
    Some(SourceGlobs { include: texts(paths, "include").unwrap_or_default(), exclude: texts(paths, "exclude") })
}

/// The texts of the list under `key`, when it is written.
fn texts(node: &Node, key: &str) -> Option<Vec<String>> {
    let items = node.get(key)?.items();
    Some(items.iter().filter_map(Node::as_str).map(str::to_owned).collect())
}

#[cfg(test)]
mod tests {
    use vc_model::{ContentHash, SourceFile, SourcePath};

    use super::SourceGlobs;

    fn file(path: &str, text: &str) -> SourceFile {
        let path = SourcePath::new(path).unwrap();
        SourceFile { path, text: text.to_owned(), hash: ContentHash::new([0; 32]) }
    }

    fn globs(config: &str) -> SourceGlobs {
        SourceGlobs::of(&[
            file("services/a/a.api.yaml", "paths: {include: [x]}"),
            file("vibranium.config.yaml", config),
        ])
    }

    #[test]
    fn reads_the_written_globs_from_the_root_config() {
        let found = globs("paths:\n  include: [services/**]\n  exclude: [legacy/**, '**/tmp']\n");
        assert_eq!(found.include, ["services/**"]);
        assert_eq!(found.exclude, Some(vec!["legacy/**".to_owned(), "**/tmp".to_owned()]));
        assert!(!found.is_default());
    }

    #[test]
    fn an_unwritten_exclude_keeps_the_defaults() {
        let found = globs("paths:\n  include: [services/**]\n");
        assert_eq!((found.include.len(), found.exclude), (1, None));
        assert_eq!(globs("paths:\n  exclude: []\n").exclude, Some(Vec::new()));
    }

    #[test]
    fn no_paths_no_config_or_a_broken_config_give_the_defaults() {
        assert!(globs("syntaxVersion: 2\n").is_default());
        assert!(globs("paths: [\n").is_default());
        assert!(SourceGlobs::of(&[file("services/a/a.api.yaml", "paths: {include: [x]}")]).is_default());
    }
}
