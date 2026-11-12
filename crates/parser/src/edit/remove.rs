//! Removing a map entry or a list item with its lines.

use std::ops::Range;

use vc_diagnostics::TextEdit;
use vc_model::SourceSpan;

use super::Document;
use crate::{Node, ParserError};

impl Document {
    /// Removes `node` with the lines it takes up: a block map entry (when `node` is its value)
    /// from its key's line to the end of its value's line, comment included; a block list item
    /// from its dash's line, together with the comment lines directly above it at the dash's
    /// indent, up to a blank line.
    ///
    /// ```
    /// use vc_model::SourceFileId;
    /// use vc_parser::{Document, Syntax};
    ///
    /// let doc = Document::parse(SourceFileId::new(0), Syntax::Yaml, "a: 1\nskip: flaky  # mine\nb: 2\n").unwrap();
    /// let edit = doc.remove(doc.root().get("skip").unwrap()).unwrap();
    /// assert_eq!(doc.apply(&[edit]).unwrap().text(), "a: 1\nb: 2\n");
    /// ```
    ///
    /// # Errors
    /// [`ParserError::BadRange`] when `node` is not part of this document,
    /// [`ParserError::WrongNode`] when it is the root, sits in a flow collection, or shares its
    /// line with what comes before it (the first key after a list item's dash), and
    /// [`ParserError::Unparsable`] when the result does not parse.
    pub fn remove(&self, node: &Node) -> Result<TextEdit, ParserError> {
        let place = self.locate(node)?;
        let range = match (place.in_flow, place.key_span, place.item) {
            (false, Some(key), _) => self.entry_range(node, key),
            (false, None, true) => self.item_range(node),
            _ => None,
        };
        let range = range.ok_or_else(|| ParserError::WrongNode {
            expected: "a block map entry or list item on lines of its own",
            found: node.kind_name(),
        })?;
        let edit = TextEdit { file: self.file, range, new_text: String::new() };
        self.apply(std::slice::from_ref(&edit)).map(|_| edit)
    }

    /// The lines of the entry whose key is at `key` and whose value is `value`.
    fn entry_range(&self, value: &Node, key: SourceSpan) -> Option<Range<usize>> {
        let start = key.start.offset;
        let end = if value.span.start.offset == value.span.end.offset { key.end.offset } else { value.span.end.offset };
        self.blank_before(start).then(|| self.line_start(start)..self.next_line(end))
    }

    /// The lines of the list item `item`, with the comment lines directly above it.
    pub(super) fn item_range(&self, item: &Node) -> Option<Range<usize>> {
        let dash = self.dash(item)?;
        let line = self.line_start(dash);
        let end = if item.span.start.offset == item.span.end.offset { dash } else { item.span.end.offset };
        Some(self.comments_above(line, dash - line)..self.next_line(end))
    }

    /// The start of the comment lines indented by `indent` spaces directly above the line
    /// starting at `line`, or `line` when there are none.
    fn comments_above(&self, mut line: usize, indent: usize) -> usize {
        while line > 0 {
            let above = self.line_start(line - 1);
            let text = self.text.get(above..line).unwrap_or_default();
            let comment = text.trim_start_matches(' ');
            if !(comment.starts_with('#') && text.len() - comment.len() == indent) {
                break;
            }
            line = above;
        }
        line
    }
}
