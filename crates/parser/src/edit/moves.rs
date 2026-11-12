//! Moving a block list item, with the comment lines above it, to another place in its list.

use std::ops::Range;

use vc_diagnostics::TextEdit;

use super::Document;
use crate::{Node, ParserError};

impl Document {
    /// Moves item `from` of the block list `seq` to place `to`, the items between moving over
    /// by one. An item moves with the lines [`Document::remove`] takes for it: its own and the
    /// comment lines directly above it at its dash's indent. Blank lines and other lines between
    /// items keep their places, and moving an item to its own place changes nothing.
    ///
    /// ```
    /// use vc_model::SourceFileId;
    /// use vc_parser::{Document, Syntax};
    ///
    /// let text = "stages:\n  - id: seed\n  # warms the cache\n  - id: load\n";
    /// let doc = Document::parse(SourceFileId::new(0), Syntax::Yaml, text).unwrap();
    /// let edit = doc.move_item(doc.root().get("stages").unwrap(), 1, 0).unwrap();
    /// let moved = "stages:\n  # warms the cache\n  - id: load\n  - id: seed\n";
    /// assert_eq!(doc.apply(&[edit]).unwrap().text(), moved);
    /// ```
    ///
    /// # Errors
    /// [`ParserError::BadRange`] when `seq` is not part of this document,
    /// [`ParserError::WrongNode`] when it is not a block list whose items are on lines of their
    /// own, or has no item at `from` or `to`, and [`ParserError::Unparsable`] when the result
    /// does not parse.
    pub fn move_item(&self, seq: &Node, from: usize, to: usize) -> Result<TextEdit, ParserError> {
        self.locate(seq)?;
        let wrong = ParserError::WrongNode {
            expected: "a block list with items on lines of their own at both places",
            found: seq.kind_name(),
        };
        let (low, high) = (from.min(to), from.max(to));
        let window: Vec<Range<usize>> = seq
            .items()
            .get(low..=high)
            .and_then(|items| items.iter().map(|item| self.item_range(item)).collect::<Option<_>>())
            .filter(|ranges: &Vec<Range<usize>>| ranges.windows(2).all(|pair| pair[0].end <= pair[1].start))
            .ok_or(wrong)?;
        let mut order: Vec<usize> = (0..window.len()).collect();
        if from < to {
            order.rotate_left(1);
        } else {
            order.rotate_right(1);
        }
        let range = window.first().zip(window.last()).map_or(0..0, |(first, last)| first.start..last.end);
        let edit = TextEdit { file: self.file, range, new_text: self.reordered(&window, &order) };
        self.apply(std::slice::from_ref(&edit)).map(|_| edit)
    }

    /// The items at `window` in `order`, each ending its last line, with the lines between them
    /// where they were; the text ends its last line only when the window did.
    fn reordered(&self, window: &[Range<usize>], order: &[usize]) -> String {
        let piece = |range: Option<Range<usize>>| range.and_then(|range| self.text.get(range)).unwrap_or_default();
        let mut out = String::new();
        for (slot, at) in order.iter().enumerate() {
            let item = piece(window.get(*at).cloned());
            out.push_str(item);
            if !item.ends_with('\n') {
                out.push_str(self.newline());
            }
            out.push_str(piece(window.get(slot).zip(window.get(slot + 1)).map(|(this, next)| this.end..next.start)));
        }
        if !piece(window.last().cloned()).ends_with('\n') {
            let kept = out.strip_suffix('\n').map_or(out.len(), |rest| rest.strip_suffix('\r').unwrap_or(rest).len());
            out.truncate(kept);
        }
        out
    }
}
