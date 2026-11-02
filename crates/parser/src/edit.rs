//! Structural changes as minimal text edits that keep comments, quoting and spacing (D107).
//!
//! Only the shape lives here for now: setting a scalar and adding a map entry. Editing proper,
//! and the engine's compile guard around it, come in M5.

use vc_diagnostics::TextEdit;
use vc_model::{SourceFileId, SourceSpan};

use crate::scalar::{Spelling, resolve, write_text};
use crate::{Node, ParserError, ScalarKind, ScalarStyle, Value, parse};

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

/// Where a node sits in its document: the key it is the value of, and whether a flow collection
/// (`[..]` or `{..}`) holds it.
#[derive(Clone, Copy, Debug, Default)]
struct Place {
    key_span: Option<SourceSpan>,
    in_flow: bool,
}

const EMPTY: ParserError =
    ParserError::WrongNode { expected: "a value, or a `key:` with nothing after it", found: "an empty value" };

impl Document {
    /// Parses `text`. An empty file is a document whose root is null.
    ///
    /// # Errors
    /// [`ParserError::Unparsable`] when the text has a parse error; warnings are accepted.
    pub fn parse(file: SourceFileId, text: impl Into<String>) -> Result<Self, ParserError> {
        let text = text.into();
        let (root, diagnostics) = parse(file, &text);
        let error = diagnostics.iter().find(|d| d.is_error()).map(|d| d.message.clone());
        match (root, error) {
            (Some(root), None) => Ok(Self { file, text, root }),
            (_, error) => Err(ParserError::Unparsable(error.unwrap_or_default())),
        }
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

    /// Replaces the plain or quoted scalar `node` with `value`, keeping its kind: text stays text
    /// in its quote style (plain text is quoted only when YAML needs it), and a number or boolean
    /// must be given as one. A null takes the kind `value` reads as. An empty value (`key:` with
    /// nothing after it) is filled in after its colon. Use [`Document::set_typed`] to change kind.
    ///
    /// # Errors
    /// As [`Document::set_typed`].
    pub fn set_scalar(&self, node: &Node, value: &str) -> Result<TextEdit, ParserError> {
        let kind = match node.as_scalar().map_or(ScalarKind::String, |scalar| scalar.kind) {
            ScalarKind::Null => resolve(value),
            kind => kind,
        };
        self.set_typed(node, value, kind)
    }

    /// Replaces the plain or quoted scalar `node` with `value` written to read back as `kind`:
    /// `set_typed(status, "201", ScalarKind::String)` writes `"201"`.
    ///
    /// # Errors
    /// [`ParserError::BadRange`] when `node` is not part of this document,
    /// [`ParserError::WrongNode`] when it is a collection, a block scalar, an empty root or an
    /// empty list item, and [`ParserError::NotOfKind`] when `value` cannot read back as `kind`.
    pub fn set_typed(&self, node: &Node, value: &str, kind: ScalarKind) -> Result<TextEdit, ParserError> {
        let place = self.locate(node)?;
        let style = match node.as_scalar() {
            Some(scalar) if !matches!(scalar.style, ScalarStyle::Literal | ScalarStyle::Folded) => scalar.style,
            _ => return Err(ParserError::WrongNode { expected: "a plain or quoted scalar", found: node.kind_name() }),
        };
        let spelling = Spelling { style, kind, flow: place.in_flow };
        let text = spelling
            .write(value)
            .ok_or_else(|| ParserError::NotOfKind { value: value.to_owned(), kind: kind.name() })?;
        if node.span.start.offset == node.span.end.offset {
            return self.fill_empty(place, &text);
        }
        Ok(TextEdit::replace(node.span, text))
    }

    /// Adds `key: value` as the last entry of the block map `map`, lined up with its keys, on the
    /// line after the last entry's content and with the file's line ending. `value` is written
    /// as text.
    ///
    /// # Errors
    /// [`ParserError::BadRange`] when `map` is not part of this document,
    /// [`ParserError::WrongNode`] when it is not a block map with at least one entry, and
    /// [`ParserError::DuplicateKey`] when it already has `key`.
    pub fn insert_entry(&self, map: &Node, key: &str, value: &str) -> Result<TextEdit, ParserError> {
        self.locate(map)?;
        let wrong = || ParserError::WrongNode { expected: "a block map with entries", found: map.kind_name() };
        let (Value::Map(entries), false) = (&map.value, self.is_flow(map)) else {
            return Err(wrong());
        };
        let (first, last) = entries.first().zip(entries.last()).ok_or_else(wrong)?;
        if entries.iter().any(|entry| entry.key == key) {
            return Err(ParserError::DuplicateKey(key.to_owned()));
        }
        let indent = " ".repeat(usize::try_from(first.key_span.start.column.saturating_sub(1)).unwrap_or(0));
        let at = self.next_line(last.value.span.end.offset);
        let newline = self.newline();
        let lead = if self.text.get(..at).is_some_and(|before| before.is_empty() || before.ends_with('\n')) {
            ""
        } else {
            newline
        };
        let new_text = format!("{lead}{indent}{}: {}{newline}", write_text(key, false), write_text(value, false));
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

    /// Finds `target` in this document's tree, so a node from another or an older document is
    /// refused instead of editing the wrong bytes.
    fn locate(&self, target: &Node) -> Result<Place, ParserError> {
        let mut stack = vec![(&self.root, Place::default())];
        while let Some((node, place)) = stack.pop() {
            if node.span == target.span && node == target {
                return Ok(place);
            }
            let in_flow = place.in_flow || self.is_flow(node);
            stack.extend(node.items().iter().map(|item| (item, Place { key_span: None, in_flow })));
            let values =
                node.entries().iter().map(|entry| (&entry.value, Place { key_span: Some(entry.key_span), in_flow }));
            stack.extend(values);
        }
        Err(ParserError::BadRange { start: target.span.start.offset, end: target.span.end.offset })
    }

    /// Whether `node` is a flow collection: `[..]` or `{..}`.
    fn is_flow(&self, node: &Node) -> bool {
        self.text.get(node.span.byte_range()).is_some_and(|text| text.starts_with(['[', '{']))
    }

    /// `text` written after the colon of an empty value's key, when only blanks or a comment
    /// follow the colon on its line: `a:  # c` becomes `a: x  # c`.
    fn fill_empty(&self, place: Place, text: &str) -> Result<TextEdit, ParserError> {
        let key_end = place.key_span.ok_or(EMPTY)?.end.offset;
        let after_key = self.text.get(key_end..).unwrap_or_default();
        let after_gap = after_key.trim_start_matches([' ', '\t']);
        let rest = after_gap.strip_prefix(':').ok_or(EMPTY)?;
        let line = rest.split('\n').next().unwrap_or_default().trim_start();
        if !(line.is_empty() || line.starts_with('#')) {
            return Err(EMPTY);
        }
        let at = key_end + (after_key.len() - after_gap.len()) + 1;
        Ok(TextEdit { file: self.file, range: at..at, new_text: format!(" {text}") })
    }

    /// The start of the line after a value ending at `offset`. A block scalar ends at the start
    /// of the next line, or after that line's indent, so that line is not skipped over; any
    /// other value ends mid-line, and the rest of its line (a comment) stays with it.
    fn next_line(&self, offset: usize) -> usize {
        let before = self.text.get(..offset).unwrap_or_default();
        let unindented = before.trim_end_matches([' ', '\t']);
        if unindented.ends_with('\n') {
            return unindented.len();
        }
        let after = self.text.get(offset..).unwrap_or_default();
        after.find('\n').map_or(self.text.len(), |i| offset + i + 1)
    }

    /// The file's line ending: `\r\n` when it uses one, `\n` otherwise.
    fn newline(&self) -> &'static str {
        if self.text.contains("\r\n") { "\r\n" } else { "\n" }
    }
}
