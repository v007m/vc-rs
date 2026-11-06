//! Renaming references to an api or a test.

use vc_diagnostics::TextEdit;

use super::Document;
use crate::{Node, ParserError, ScalarKind};

/// Keys whose value is a reference to an api or a test.
const REFERENCES: [&str; 2] = ["call", "test"];

/// Keys whose value is data sent or checked, never a definition holding references.
const DATA: [&str; 8] = ["vars", "body", "headers", "query", "form", "expect", "save", "examples"];

/// A rename of the full ID `from` to `to`, seen from the file `file_id`.
struct Rename<'a> {
    file_id: &'a str,
    from: &'a str,
    to: &'a str,
}

impl Document {
    /// Edits that rename every `call` and `test` reference to the full ID `from` (`users#create`)
    /// into `to`, in this file, whose ID is `file_id`. A short reference (`create`, which names
    /// an item of this file) stays short when `to` is in this file too; any other is written in
    /// full. Quote style and comments are kept, definitions (`id:`) are not touched, and data
    /// (`vars`, `body`, `expect`, ...) is not looked into.
    ///
    /// ```
    /// use vc_model::SourceFileId;
    /// use vc_parser::{Document, Syntax};
    ///
    /// let text = "tests:\n  - id: a\n    call: 'users#create'  # api\n";
    /// let doc = Document::parse(SourceFileId::new(0), Syntax::Yaml, text).unwrap();
    /// let edits = doc.rename_refs("users.crud", "users#create", "users#createUser").unwrap();
    /// let after = doc.apply(&edits).unwrap();
    /// assert_eq!(after.text(), "tests:\n  - id: a\n    call: 'users#createUser'  # api\n");
    /// ```
    ///
    /// # Errors
    /// [`ParserError::Unparsable`] when an edited reference would not parse.
    pub fn rename_refs(&self, file_id: &str, from: &str, to: &str) -> Result<Vec<TextEdit>, ParserError> {
        let rename = Rename { file_id, from, to };
        references(&self.root)
            .into_iter()
            .filter_map(|node| node.as_str().and_then(|text| rename.renamed(text)).map(|text| (node, text)))
            .map(|(node, text)| self.set_typed(node, &text, ScalarKind::String))
            .collect()
    }
}

impl Rename<'_> {
    /// The new text of the reference `text`, when it names `from`.
    fn renamed(&self, text: &str) -> Option<String> {
        let short = !text.contains('#');
        let full = if short { format!("{}#{text}", self.file_id) } else { text.to_owned() };
        if full != self.from {
            return None;
        }
        Some(match self.to.split_once('#') {
            Some((file, id)) if short && file == self.file_id => id.to_owned(),
            _ => self.to.to_owned(),
        })
    }
}

/// Every text value of a `call` or `test` key under `node`, in file order, without looking into
/// data (`vars`, `body`, `expect`, ...): the references to an api or a test.
#[must_use]
pub fn references(node: &Node) -> Vec<&Node> {
    let mut found = Vec::new();
    collect(node, &mut found);
    found
}

fn collect<'n>(node: &'n Node, found: &mut Vec<&'n Node>) {
    for item in node.items() {
        collect(item, found);
    }
    for entry in node.entries() {
        let key = entry.key.as_str();
        if REFERENCES.contains(&key) && entry.value.as_str().is_some() {
            found.push(&entry.value);
        } else if !DATA.contains(&key) {
            collect(&entry.value, found);
        }
    }
}
