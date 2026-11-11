//! Renaming a word in comment lines: the IDs that the comments `vc new` writes name.

use vc_diagnostics::TextEdit;

use super::Document;
use super::format_tree::multi_line_scalars;

impl Document {
    /// Edits that replace each word `from` with `to` in the comment lines that `chosen` picks
    /// (given the line from its `#`, trimmed). A word is text between blanks or the line's ends;
    /// a line inside a scalar written over several lines is not a comment.
    ///
    /// ```
    /// use vc_model::SourceFileId;
    /// use vc_parser::{Document, Syntax};
    ///
    /// let text = "apis:\n  # users#list · GET /users\n  - id: list\n    path: /users  # users#list\n";
    /// let doc = Document::parse(SourceFileId::new(0), Syntax::Yaml, text).unwrap();
    /// let edits = doc.rename_in_comments(("users#list", "users#listAll"), &|comment| comment.contains(" · "));
    /// let after = doc.apply(&edits).unwrap();
    /// assert_eq!(after.text(), text.replacen("# users#list ·", "# users#listAll ·", 1));
    /// ```
    #[must_use]
    pub fn rename_in_comments(&self, (from, to): (&str, &str), chosen: &dyn Fn(&str) -> bool) -> Vec<TextEdit> {
        let mut scalars = Vec::new();
        multi_line_scalars(self, &self.root, &mut scalars);
        let mut edits = Vec::new();
        let mut start = 0;
        for line in self.text.split_inclusive('\n') {
            let at = start;
            start += line.len();
            let comment = line.trim();
            let in_scalar = scalars.iter().any(|span| span.start < at && at < span.end);
            if from.is_empty() || !comment.starts_with('#') || in_scalar || !chosen(comment) {
                continue;
            }
            for (offset, _) in
                line.match_indices(from).filter(|(offset, _)| is_word(line, *offset..*offset + from.len()))
            {
                edits.push(TextEdit {
                    file: self.file,
                    range: at + offset..at + offset + from.len(),
                    new_text: to.to_owned(),
                });
            }
        }
        edits
    }
}

/// Whether the bytes `range` of `line` are a whole word: blanks or the line's ends around them.
fn is_word(line: &str, range: std::ops::Range<usize>) -> bool {
    let before = line.get(..range.start).and_then(|text| text.chars().next_back());
    let after = line.get(range.end..).and_then(|text| text.chars().next());
    before.is_none_or(char::is_whitespace) && after.is_none_or(char::is_whitespace)
}
