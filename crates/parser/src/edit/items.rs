//! New list items and new entries given as YAML fragments.

use vc_diagnostics::TextEdit;

use super::{Document, Place, indent};
use crate::scalar::Spelling;
use crate::{Node, ParserError, Syntax, Value, scalar};

const NOT_A_LIST: ParserError =
    ParserError::WrongNode { expected: "a YAML block list, or a flow list of scalars", found: "something else" };

impl Document {
    /// Adds the YAML fragment `yaml` as the last item of the list `seq`. In a block list it goes
    /// on the lines after the last item, lined up with its dashes; in a flow list it is a scalar
    /// written after the last one (`[a, b]` becomes `[a, b, c]`). An empty flow list that is a
    /// key's value and gets a collection becomes a block list on the lines below the key.
    ///
    /// ```
    /// use vc_model::SourceFileId;
    /// use vc_parser::{Document, Syntax};
    ///
    /// let doc = Document::parse(SourceFileId::new(0), Syntax::Yaml, "tags: [smoke]  # keep\n").unwrap();
    /// let edit = doc.insert_item(doc.root().get("tags").unwrap(), "slow-network").unwrap();
    /// assert_eq!(doc.apply(&[edit]).unwrap().text(), "tags: [smoke, slow-network]  # keep\n");
    /// ```
    ///
    /// # Errors
    /// [`ParserError::BadRange`] when `seq` is not part of this document,
    /// [`ParserError::Unparsable`] when `yaml` or the result does not parse, and
    /// [`ParserError::WrongNode`] when `seq` is not a YAML list, or is a flow list given a
    /// collection it cannot hold.
    pub fn insert_item(&self, seq: &Node, yaml: &str) -> Result<TextEdit, ParserError> {
        let place = self.locate(seq)?;
        let (Value::Seq(items), Syntax::Yaml) = (&seq.value, self.syntax) else {
            return Err(NOT_A_LIST);
        };
        let item = self.fragment(yaml)?;
        let edit = match (self.is_flow(seq), item.as_scalar()) {
            (false, _) => self.block_item(items, yaml)?,
            (true, Some(_)) => self.flow_item(seq, items.last(), &item)?,
            (true, None) if items.is_empty() => self.unfold(seq, place, yaml)?,
            (true, None) => return Err(NOT_A_LIST),
        };
        self.apply(std::slice::from_ref(&edit)).map(|_| edit)
    }

    /// Adds `key:` with the YAML fragment `yaml` as the last entry of the block map `map`: a
    /// scalar after the colon as written, a collection on the lines below, two spaces deeper.
    ///
    /// # Errors
    /// As [`Document::insert_entry`], and [`ParserError::Unparsable`] when `yaml` or the result
    /// does not parse.
    pub fn insert_yaml(&self, map: &Node, key: &str, yaml: &str) -> Result<TextEdit, ParserError> {
        let (first, last) = self.block_entries(map, key)?;
        let (at, lead) = self.line_after(last.value.span.end.offset);
        let indent = indent(first.key_span);
        let key = scalar::write_text(key, false);
        let body = match self.fragment(yaml)?.as_scalar() {
            Some(_) => format!("{indent}{key}: {}{}", yaml.trim(), self.newline()),
            None => format!("{indent}{key}:{}{}", self.newline(), self.lines(&format!("{indent}  "), ["", ""], yaml)),
        };
        let edit = TextEdit { file: self.file, range: at..at, new_text: format!("{lead}{body}") };
        self.apply(std::slice::from_ref(&edit)).map(|_| edit)
    }

    /// Adds `key: value` (written as text) as the first entry of the block map `map`, on the
    /// line of its first key: for the root, after any comment lines at the top of the file.
    ///
    /// # Errors
    /// As [`Document::insert_entry`].
    pub fn insert_first(&self, map: &Node, key: &str, value: &str) -> Result<TextEdit, ParserError> {
        let (first, _) = self.block_entries(map, key)?;
        let at = first.key_span.start.offset;
        if !self.blank_before(at) {
            return Err(ParserError::WrongNode {
                expected: "a map whose first key starts a line",
                found: "a list item",
            });
        }
        let at = self.line_start(at);
        let line = format!("{}: {}", scalar::write_text(key, false), scalar::write_text(value, false));
        let new_text = format!("{}{line}{}", indent(first.key_span), self.newline());
        Ok(TextEdit { file: self.file, range: at..at, new_text })
    }

    /// `yaml` read as a fragment of this file.
    fn fragment(&self, yaml: &str) -> Result<Node, ParserError> {
        let (node, diagnostics) = crate::parse(self.file, Syntax::Yaml, yaml);
        let error = diagnostics.iter().find(|d| d.is_error()).map(|d| d.message.clone());
        match (node, error) {
            (Some(node), None) => Ok(node),
            (_, error) => Err(ParserError::Unparsable(error.unwrap_or_default())),
        }
    }

    /// `yaml` as a new last item of the block list `items`.
    fn block_item(&self, items: &[Node], yaml: &str) -> Result<TextEdit, ParserError> {
        let (first, last) = items.first().zip(items.last()).ok_or(NOT_A_LIST)?;
        let dash = self.dash(first).ok_or(NOT_A_LIST)?;
        let indent = " ".repeat(dash - self.line_start(dash));
        let (at, lead) = self.line_after(last.span.end.offset);
        let new_text = format!("{lead}{}", self.lines(&indent, ["- ", "  "], yaml));
        Ok(TextEdit { file: self.file, range: at..at, new_text })
    }

    /// The scalar `item` written into the flow list `seq`, after `last` or as its only item.
    fn flow_item(&self, seq: &Node, last: Option<&Node>, item: &Node) -> Result<TextEdit, ParserError> {
        let scalar = item.as_scalar().ok_or(NOT_A_LIST)?;
        let spelling = Spelling { syntax: Syntax::Yaml, style: scalar.style, kind: scalar.kind, flow: true };
        let written = spelling.write(&scalar.text).ok_or(NOT_A_LIST)?;
        let (at, new_text) = match last {
            Some(last) => (last.span.end.offset, format!(", {written}")),
            None => (seq.span.end.offset.saturating_sub(1), written),
        };
        Ok(TextEdit { file: self.file, range: at..at, new_text })
    }

    /// The empty flow list `seq` (a key's value) replaced by a block list holding `yaml`, on the
    /// lines below the key and two spaces deeper.
    fn unfold(&self, seq: &Node, place: Place, yaml: &str) -> Result<TextEdit, ParserError> {
        let key = place.key_span.ok_or(NOT_A_LIST)?;
        let between = self.text.get(key.end.offset..seq.span.start.offset).ok_or(NOT_A_LIST)?;
        let colon = key.end.offset + between.find(':').ok_or(NOT_A_LIST)? + 1;
        let newline = self.newline();
        let lines = self.lines(&format!("{}  ", indent(key)), ["- ", "  "], yaml);
        let new_text = format!("{newline}{}", lines.strip_suffix(newline).unwrap_or(&lines));
        Ok(TextEdit { file: self.file, range: colon..seq.span.end.offset, new_text })
    }

    /// The lines of `yaml` after `indent`, the first one after `marks[0]` and the rest after
    /// `marks[1]`, each ending with the file's line ending; blank lines stay blank.
    fn lines(&self, indent: &str, marks: [&str; 2], yaml: &str) -> String {
        let newline = self.newline();
        let lines = yaml.trim_end_matches(['\r', '\n']).lines().enumerate();
        lines
            .map(|(i, line)| match (line.trim().is_empty(), i) {
                (true, _) => newline.to_owned(),
                (false, 0) => format!("{indent}{}{line}{newline}", marks[0]),
                (false, _) => format!("{indent}{}{line}{newline}", marks[1]),
            })
            .collect()
    }

    /// The offset of the `-` that starts the block list item `item`, when only spaces come
    /// before it on its line.
    pub(super) fn dash(&self, item: &Node) -> Option<usize> {
        let start = item.span.start.offset;
        let is_dash = |at: usize| {
            self.text.get(at..).is_some_and(|rest| rest.starts_with('-'))
                && self.text.get(at + 1..).is_none_or(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        };
        let at = if is_dash(start) {
            start
        } else {
            self.text.get(..start)?.trim_end_matches([' ', '\t', '\r', '\n']).len().checked_sub(1)?
        };
        (is_dash(at) && self.blank_before(at)).then_some(at)
    }
}
