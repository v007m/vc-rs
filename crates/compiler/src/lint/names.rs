//! V4003 and V4004: file IDs, tags and profiles that break the naming conventions.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link::{self, Source};
use crate::naming;

/// V4003: a file ID word that is not lower case (`services/users/CRUD.test.yaml`).
pub struct FileNames;

/// V4004: a tag that is not lower kebab-case, or a profile that is not a lower-case word.
pub struct Labels;

impl Lint for FileNames {
    fn code(&self) -> Code {
        Code::V4003
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        cx.project.sources.iter().filter_map(|source| self.file(source)).collect()
    }
}

impl FileNames {
    fn file(&self, source: &Source<'_>) -> Option<Diagnostic> {
        let id = source.id.as_deref()?;
        if naming::is_file_id(id) {
            return None;
        }
        let spot = source.root.entries().first().map_or(source.root.span, |entry| entry.key_span);
        let message = format!("the file ID `{id}` is not lower case");
        let diagnostic = Diagnostic::new(self.code(), spot, message).label("in this file's path");
        let help = format!("move it with `vc rename {id} {}`, which updates every reference", naming::file_id(id));
        Some(diagnostic.help(help))
    }
}

impl Lint for Labels {
    fn code(&self) -> Code {
        Code::V4004
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let tagged = cx.test_files().flat_map(|source| {
            let tests = link::list(source.root, "tests").iter();
            std::iter::once(source.root).chain(tests)
        });
        let tags = tagged.flat_map(|node| link::list(node, "tags").iter());
        let mut found: Vec<Diagnostic> = tags.filter_map(|tag| self.tag(tag)).collect();
        if let Some(config) = cx.project.config() {
            let profiles = link::list(config.root, "profiles").iter();
            found.extend(profiles.filter_map(|profile| self.profile(profile.get("id")?)));
        }
        found
    }
}

impl Labels {
    fn tag(&self, node: &Node) -> Option<Diagnostic> {
        let written = node.as_str()?;
        if naming::is_tag(written) {
            return None;
        }
        let better = naming::tag(written);
        let message = format!("tag `{written}` is not lower kebab-case");
        let diagnostic = Diagnostic::new(self.code(), node.span, message).label("rename it");
        if !naming::is_tag(&better) || node.span.byte_range().len() != written.len() {
            return Some(diagnostic.help("tags are lower case, with a dash between words: `slow-network`"));
        }
        let diagnostic = diagnostic.help(format!("tags are lower case, with a dash between words: `{better}`"));
        let edit = TextEdit::replace(node.span, better.clone());
        Some(diagnostic.fix(Fix::certain(format!("Rename to {better}"), vec![edit])))
    }

    fn profile(&self, node: &Node) -> Option<Diagnostic> {
        let written = node.as_str()?;
        if naming::is_lower_word(written, false) {
            return None;
        }
        let message = format!("profile `{written}` is not a lower-case word");
        let diagnostic = Diagnostic::new(self.code(), node.span, message).label("rename it");
        let better = naming::tag(written).replace('-', "");
        Some(diagnostic.help(format!("profiles are lower-case single words: `{better}`, also where it is used")))
    }
}
