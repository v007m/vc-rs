//! Structural changes as minimal text edits that keep comments, quoting and spacing (D107).
//!
//! Only the shape lives here for now: setting a scalar and adding a map entry. Editing proper,
//! and the engine's compile guard around it, come in M5.

use vc_diagnostics::TextEdit;
use vc_model::SourceFileId;

use crate::scalar::render;
use crate::{Node, ParserError, ScalarStyle, Value, parse};

/// A parsed file kept with its text, so changes to its tree become edits on that text.
///
/// ```
/// use vc_model::SourceFileId;
/// use vc_parser::Document;
///
/// let doc = Document::parse(SourceFileId::new(0), "id: getsUser  # keep me\nname: Gets a user\n").unwrap();
/// let edit = doc.set_scalar(doc.root().get("name").unwrap(), "Gets one user").unwrap();
/// let changed = doc.apply(&[edit]).unwrap();
/// assert_eq!(changed.text(), "id: getsUser  # keep me\nname: Gets one user\n");
/// ```
#[derive(Clone, Debug)]
pub struct Document {
    file: SourceFileId,
    text: String,
    root: Node,
}

impl Document {
    /// Parses `text`.
    ///
    /// # Errors
    /// [`ParserError::Unparsable`] when the text has a parse error; warnings are accepted.
    pub fn parse(file: SourceFileId, text: impl Into<String>) -> Result<Self, ParserError> {
        let text = text.into();
        let (root, diagnostics) = parse(file, &text);
        if let Some(error) = diagnostics.iter().find(|d| d.is_error()) {
            return Err(ParserError::Unparsable(error.message.clone()));
        }
        let root = root.ok_or_else(|| ParserError::Unparsable("no document".to_owned()))?;
        Ok(Self { file, text, root })
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The tree; pass its nodes back to the edit methods.
    #[must_use]
    pub const fn root(&self) -> &Node {
        &self.root
    }

    /// Replaces the plain or quoted scalar `node` with `value`, quoted only if YAML needs it.
    ///
    /// # Errors
    /// [`ParserError::WrongNode`] when `node` is a collection or a block scalar.
    pub fn set_scalar(&self, node: &Node, value: &str) -> Result<TextEdit, ParserError> {
        match node.as_scalar() {
            Some(scalar) if !matches!(scalar.style, ScalarStyle::Literal | ScalarStyle::Folded) => {
                Ok(TextEdit::replace(node.span, render(value)))
            }
            _ => Err(ParserError::WrongNode { expected: "a plain or quoted scalar", found: node.kind_name() }),
        }
    }

    /// Adds `key: value` as the last entry of the block map `map`, lined up with its keys.
    ///
    /// # Errors
    /// [`ParserError::WrongNode`] when `map` is not a block map with at least one entry.
    pub fn insert_entry(&self, map: &Node, key: &str, value: &str) -> Result<TextEdit, ParserError> {
        let wrong = || ParserError::WrongNode { expected: "a block map with entries", found: map.kind_name() };
        let flow = self.text.get(map.span.byte_range()).is_none_or(|text| text.starts_with('{'));
        let (Value::Map(entries), false) = (&map.value, flow) else {
            return Err(wrong());
        };
        let (first, last) = entries.first().zip(entries.last()).ok_or_else(wrong)?;
        let indent = " ".repeat(usize::try_from(first.key_span.start.column.saturating_sub(1)).unwrap_or(0));
        let at = self.line_end(last.value.span.end.offset);
        let line_start = self.text.get(..at).is_some_and(|before| before.is_empty() || before.ends_with('\n'));
        let lead = if line_start { "" } else { "\n" };
        let new_text = format!("{lead}{indent}{}: {}\n", render(key), render(value));
        Ok(TextEdit { file: self.file, range: at..at, new_text })
    }

    /// The text with `edits` applied, parsed again.
    ///
    /// # Errors
    /// [`ParserError::BadRange`] when an edit does not fit the text or overlaps another, and
    /// [`ParserError::Unparsable`] when the result does not parse.
    pub fn apply(&self, edits: &[TextEdit]) -> Result<Self, ParserError> {
        let mut sorted: Vec<&TextEdit> = edits.iter().collect();
        sorted.sort_by_key(|edit| (edit.range.start, edit.range.end));
        let mut out = String::with_capacity(self.text.len());
        let mut cursor = 0;
        for edit in sorted {
            let (start, end) = (edit.range.start, edit.range.end);
            let kept =
                self.text.get(cursor..start).filter(|_| edit.file == self.file && self.text.get(start..end).is_some());
            out.push_str(kept.ok_or(ParserError::BadRange { start, end })?);
            out.push_str(&edit.new_text);
            cursor = end;
        }
        out.push_str(self.text.get(cursor..).unwrap_or_default());
        Self::parse(self.file, out)
    }

    /// The offset just after the line break that ends the line holding `offset`.
    fn line_end(&self, offset: usize) -> usize {
        let before = self.text.get(..offset).unwrap_or_default();
        if before.ends_with('\n') {
            return offset;
        }
        let after = self.text.get(offset..).unwrap_or_default();
        after.find('\n').map_or(self.text.len(), |i| offset + i + 1)
    }
}
