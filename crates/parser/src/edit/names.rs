//! Renaming a var, an auth or a dataset: the names that templates, CEL and `auth` keys use.

use std::ops::Range;

use vc_diagnostics::TextEdit;
use vc_model::SourceSpan;

use super::Document;
use crate::{Node, ParserError, ScalarKind, scalar};

/// Keys whose map defines vars: each of its keys is a var's name.
const VAR_MAPS: [&str; 2] = ["vars", "save"];

/// Keys whose text, or whose map's texts, are CEL: every name in them is a reference.
const CEL_KEYS: [&str; 2] = ["cel", "$cel"];

/// A root and name as templates write them (`var.userId`), and what it becomes.
struct Token<'a> {
    from: &'a str,
    to: &'a str,
    /// Whether a path may follow the name (`var.user.id`); a dataset's dotted ID takes none,
    /// so `dataset.users` is not part of `dataset.users.admins`.
    paths: bool,
}

impl Document {
    /// Edits that rename the var `from` to `to`: the keys of every `vars` and `save` map, and
    /// every `var.<from>` in a `${…}` reference (with or without a path after it) or in CEL
    /// (`expect.cel`, `$cel`). Everything else, quoting and comments included, is kept; a
    /// literal `$${…}` is not a reference.
    ///
    /// ```
    /// use vc_model::SourceFileId;
    /// use vc_parser::{Document, Syntax};
    ///
    /// let text = "vars: {userId: 1}\npath: /users/${var.userId}\n";
    /// let doc = Document::parse(SourceFileId::new(0), Syntax::Yaml, text).unwrap();
    /// let after = doc.apply(&doc.rename_var("userId", "accountId").unwrap()).unwrap();
    /// assert_eq!(after.text(), "vars: {accountId: 1}\npath: /users/${var.accountId}\n");
    /// ```
    ///
    /// # Errors
    /// [`ParserError::Unparsable`] when the renamed text would not parse.
    pub fn rename_var(&self, from: &str, to: &str) -> Result<Vec<TextEdit>, ParserError> {
        let mut spans = Vec::new();
        var_keys(&self.root, from, &mut spans);
        let mut edits: Vec<TextEdit> =
            spans.into_iter().map(|span| TextEdit::replace(span, scalar::write_text(to, false))).collect();
        let (from, to) = (format!("var.{from}"), format!("var.{to}"));
        edits.extend(self.renamed_tokens(&Token { from: &from, to: &to, paths: true }));
        self.checked(edits)
    }

    /// Edits that rename every `dataset.<from>` reference to `dataset.<to>`, in templates and
    /// CEL; `dataset.<from>.<more>` names another dataset and is left alone.
    ///
    /// # Errors
    /// [`ParserError::Unparsable`] when the renamed text would not parse.
    pub fn rename_dataset(&self, from: &str, to: &str) -> Result<Vec<TextEdit>, ParserError> {
        let (from, to) = (format!("dataset.{from}"), format!("dataset.{to}"));
        let edits = self.renamed_tokens(&Token { from: &from, to: &to, paths: false });
        self.checked(edits)
    }

    /// Edits that rename every `auth` that names the auth `from` (`auth: from`, or the `id` of
    /// `auth: {id: from, vars: …}`) to `to`, keeping its quote style. The definitions in the
    /// config's `auth` list are not references and are left alone.
    ///
    /// # Errors
    /// [`ParserError::Unparsable`] when the renamed text would not parse.
    pub fn rename_auth(&self, from: &str, to: &str) -> Result<Vec<TextEdit>, ParserError> {
        let mut found = Vec::new();
        auths(&self.root, &mut found);
        let named = found.into_iter().filter(|node| node.as_str() == Some(from));
        named.map(|node| self.set_typed(node, to, ScalarKind::String)).collect()
    }

    /// The edits that write `token.to` over each `token.from` in a text scalar of this document.
    /// The match is made on the source text, where a name is spelled the same in any style.
    fn renamed_tokens(&self, token: &Token<'_>) -> Vec<TextEdit> {
        let mut texts = Vec::new();
        text_nodes(&self.root, false, &mut texts);
        let mut edits = Vec::new();
        for (node, cel) in texts {
            let start = node.span.start.offset;
            let raw = self.text.get(start..node.span.end.offset).unwrap_or_default();
            edits.extend(matches(raw, token, cel).into_iter().map(|range| TextEdit {
                file: self.file,
                range: start + range.start..start + range.end,
                new_text: token.to.to_owned(),
            }));
        }
        edits
    }

    /// `edits`, once the text they give parses.
    fn checked(&self, edits: Vec<TextEdit>) -> Result<Vec<TextEdit>, ParserError> {
        self.apply(&edits).map(|_| edits)
    }
}

/// The span of every key `from` in a `vars` or `save` map under `node`.
fn var_keys(node: &Node, from: &str, found: &mut Vec<SourceSpan>) {
    for item in node.items() {
        var_keys(item, from, found);
    }
    for entry in node.entries() {
        if VAR_MAPS.contains(&entry.key.as_str()) {
            found.extend(entry.value.entries().iter().filter(|var| var.key == from).map(|var| var.key_span));
        }
        var_keys(&entry.value, from, found);
    }
}

/// Every text value of an `auth` key under `node`, and the `id` of every map one.
fn auths<'n>(node: &'n Node, found: &mut Vec<&'n Node>) {
    for item in node.items() {
        auths(item, found);
    }
    for entry in node.entries() {
        if entry.key == "auth" {
            let named = Some(&entry.value).filter(|value| value.as_str().is_some());
            found.extend(named.or_else(|| entry.value.get("id")));
        }
        auths(&entry.value, found);
    }
}

/// Every text scalar under `node`, each with whether it is CEL.
fn text_nodes<'n>(node: &'n Node, cel: bool, found: &mut Vec<(&'n Node, bool)>) {
    if node.as_str().is_some() {
        found.push((node, cel));
    }
    for item in node.items() {
        text_nodes(item, cel, found);
    }
    for entry in node.entries() {
        text_nodes(&entry.value, cel || CEL_KEYS.contains(&entry.key.as_str()), found);
    }
}

/// Where in `raw` the token is written as a name: inside a `${…}` reference, or anywhere when
/// `raw` is CEL.
fn matches(raw: &str, token: &Token<'_>, cel: bool) -> Vec<Range<usize>> {
    let spans = if cel { vec![0..raw.len()] } else { references(raw) };
    let mut found = Vec::new();
    for span in spans {
        let Some(part) = raw.get(span.clone()) else { continue };
        let at = part.match_indices(token.from).map(|(at, _)| span.start + at..span.start + at + token.from.len());
        found.extend(at.filter(|range| token.stands_alone(raw, range)));
    }
    found
}

/// The range of each `${…}` in `raw`; `$${` writes a literal `${` and starts none.
fn references(raw: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(start) = raw.get(from..).and_then(|rest| rest.find("${")).map(|at| from + at) {
        let end = raw.get(start..).and_then(|rest| rest.find('}')).map_or(raw.len(), |at| start + at);
        if !raw.get(..start).is_some_and(|before| before.ends_with('$')) {
            found.push(start..end);
        }
        from = end;
    }
    found
}

impl Token<'_> {
    /// Whether the match at `range` is the whole name: not the end of a longer word or path,
    /// and not the start of a longer name.
    fn stands_alone(&self, raw: &str, range: &Range<usize>) -> bool {
        let before = raw.get(..range.start).and_then(|text| text.chars().next_back());
        let after = raw.get(range.end..).and_then(|text| text.chars().next());
        let joined = |c: char| c.is_ascii_alphanumeric() || c == '_';
        !before.is_some_and(|c| joined(c) || c == '.') && !after.is_some_and(|c| joined(c) || (c == '.' && !self.paths))
    }
}
