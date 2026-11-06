//! A whole YAML document written again in the house style (D180): block collections with a
//! 2-space indent, list items under their key, the keys of a declared map in the order the rules
//! give, text plain unless YAML needs quotes. Comment lines move with the entry or item below
//! them and a comment at the end of a line stays on it; the result must read back as the same
//! values and keep every comment line, or nothing is written.

use std::collections::HashSet;
use std::ops::Range;

use super::Document;
use crate::{Entry, Node, ParserError, ScalarKind, ScalarStyle, Syntax, Value, scalar};

/// What the house style takes from the caller.
pub trait FormatRules {
    /// The keys of the map at `path` (each segment a key, or `-` for a list item) in the order
    /// to write them; keys it leaves out go last, in the author's order. `None` keeps the
    /// author's order.
    fn key_order(&self, path: &[&str]) -> Option<Vec<&'static str>>;

    /// Whether the comment `comment` (from its `#`) is left out.
    fn drops(&self, comment: &str) -> bool;
}

/// Where a value goes: the indent of its children, and the offset after its key's colon or its
/// item's dash.
#[derive(Clone, Copy)]
struct At {
    indent: usize,
    after: usize,
}

/// The formatted text, written as the tree is walked.
struct Writer<'d> {
    doc: &'d Document,
    rules: &'d dyn FormatRules,
    out: String,
    path: Vec<&'d str>,
    scalars: Vec<Range<usize>>,
}

impl Document {
    /// This YAML document in the house style; a JSON document, or one whose root is not a map
    /// with entries, is given back as it is.
    ///
    /// ```
    /// use vc_model::SourceFileId;
    /// use vc_parser::{Document, FormatRules, Syntax};
    ///
    /// struct Rules;
    /// impl FormatRules for Rules {
    ///     fn key_order(&self, path: &[&str]) -> Option<Vec<&'static str>> {
    ///         path.is_empty().then(|| vec!["id", "name"])
    ///     }
    ///     fn drops(&self, _: &str) -> bool {
    ///         false
    ///     }
    /// }
    /// let text = "# mine\nname: 'Gets'  # keep\nid: get\nvars: {b: 1}\n";
    /// let doc = Document::parse(SourceFileId::new(0), Syntax::Yaml, text).unwrap();
    /// assert_eq!(doc.format(&Rules).unwrap(), "# mine\nid: get\nname: Gets  # keep\nvars:\n  b: 1\n");
    /// ```
    ///
    /// # Errors
    /// [`ParserError::Unformattable`] when the result would read as other values or lose a
    /// comment line (one inside a flow collection spread over lines, say).
    pub fn format(&self, rules: &dyn FormatRules) -> Result<String, ParserError> {
        let entries = match (self.syntax, &self.root.value) {
            (Syntax::Yaml, Value::Map(entries)) if !entries.is_empty() => entries,
            _ => return Ok(self.text.clone()),
        };
        let mut scalars = Vec::new();
        multi_line_scalars(self, &self.root, &mut scalars);
        let mut writer = Writer { doc: self, rules, out: String::new(), path: Vec::new(), scalars };
        writer.header();
        writer.map(entries, 0);
        writer.tail();
        let comments = writer.comment_lines();
        let formatted = Self::parse(self.file, self.syntax, writer.out)
            .map_err(|error| ParserError::Unformattable(format!("the result would not parse ({error})")))?;
        if !same(&self.root, &formatted.root) {
            return Err(ParserError::Unformattable("the result would read as other values".to_owned()));
        }
        let kept: HashSet<&str> = formatted.text.lines().map(str::trim).collect();
        match comments.into_iter().find(|comment| !kept.contains(comment)) {
            Some(lost) => Err(ParserError::Unformattable(format!("it would lose the comment `{lost}`"))),
            None => Ok(formatted.text),
        }
    }
}

impl<'d> Writer<'d> {
    /// The comment lines above the first content line, at the top.
    fn header(&mut self) {
        let lines = self.doc.text.lines().map(str::trim);
        let header: Vec<&str> = lines.take_while(|line| line.is_empty() || line.starts_with('#')).collect();
        self.comments(&header, 0);
    }

    /// The comment lines after the last content line, at the end.
    fn tail(&mut self) {
        let start = self.doc.next_line(self.doc.root.span.end.offset);
        let rest = self.doc.text.get(start..).unwrap_or_default();
        let lines = rest.lines().map(str::trim);
        let mut tail: Vec<&str> = lines.filter(|line| line.is_empty() || line.starts_with('#')).collect();
        while tail.last().is_some_and(|line| line.is_empty()) {
            tail.pop();
        }
        self.comments(&tail, 0);
    }

    /// The entries of a map, each on lines of its own at `indent`.
    fn map(&mut self, entries: &'d [Entry], indent: usize) {
        for entry in self.ordered(entries) {
            self.own_lines(entry, indent);
        }
    }

    /// `entry` with the comment lines above it, at `indent`.
    fn own_lines(&mut self, entry: &'d Entry, indent: usize) {
        let above = self.above_entry(entry);
        self.comments(&above, indent);
        self.pad(indent);
        self.entry(entry, indent);
    }

    /// `key:` and its value, from where the caller left the line.
    fn entry(&mut self, entry: &'d Entry, indent: usize) {
        let written = self.doc.text.get(entry.key_span.byte_range()).filter(|key| !key.is_empty() && !key.contains('\n'));
        let key = written.map_or_else(|| scalar::write_text(&entry.key, false), str::to_owned);
        self.out.push_str(&key);
        self.out.push(':');
        let after = self.colon_end(entry.key_span.end.offset);
        self.path.push(&entry.key);
        self.value(&entry.value, At { indent: indent + 2, after });
        self.path.pop();
    }

    /// Each item of a list, at `indent`.
    fn seq(&mut self, items: &'d [Node], indent: usize) {
        for item in items {
            self.item(item, indent);
        }
    }

    /// One list item: `- ` and a map's first entry on the dash's line, or `-` and its value.
    fn item(&mut self, item: &'d Node, indent: usize) {
        let dash = self.doc.dash(item);
        let mut above = dash.map(|dash| self.above(dash)).unwrap_or_default();
        self.path.push("-");
        let mut entries = self.ordered(item.entries()).into_iter();
        if let Some(first) = entries.next() {
            above.extend(self.above_entry(first));
            self.comments(&above, indent);
            self.pad(indent);
            self.out.push_str("- ");
            self.entry(first, indent + 2);
            entries.for_each(|entry| self.own_lines(entry, indent + 2));
        } else {
            self.comments(&above, indent);
            self.pad(indent);
            self.out.push('-');
            let after = dash.map_or(item.span.start.offset, |dash| dash + 1);
            self.value(item, At { indent: indent + 2, after });
        }
        self.path.pop();
    }

    /// `node` after its key's colon or its item's dash: a collection on the lines below, two
    /// spaces deeper, and anything else on the same line.
    fn value(&mut self, node: &'d Node, at: At) {
        let block = !self.doc.is_flow(node) && node.span.start.offset < node.span.end.offset;
        let line_end = if block { at.after } else { node.span.end.offset };
        match &node.value {
            Value::Map(entries) if !entries.is_empty() => {
                self.end_line(line_end);
                self.map(entries, at.indent);
            }
            Value::Seq(items) if !items.is_empty() => {
                self.end_line(line_end);
                self.seq(items, at.indent);
            }
            _ => self.inline(node, at),
        }
    }

    /// A scalar or an empty collection on the line of its key or dash, with its line comment; a
    /// block scalar's comment stays on its header line.
    fn inline(&mut self, node: &Node, at: At) {
        let text = self.scalar(node, at.indent);
        if text.is_empty() {
            self.end_line(at.after);
            return;
        }
        self.out.push(' ');
        self.out.push_str(&text);
        if text.contains('\n') {
            self.out.push_str(self.doc.newline());
        } else {
            self.end_line(node.span.end.offset);
        }
    }

    /// A scalar or an empty collection as text: a number, boolean or null as written, text in
    /// the house style, a block scalar indented to `indent`.
    fn scalar(&self, node: &Node, indent: usize) -> String {
        let source = self.doc.text.get(node.span.byte_range()).unwrap_or_default();
        match &node.value {
            Value::Map(_) => "{}".to_owned(),
            Value::Seq(_) => "[]".to_owned(),
            Value::Scalar(scalar) if scalar.kind != ScalarKind::String => source.to_owned(),
            Value::Scalar(scalar) if matches!(scalar.style, ScalarStyle::Literal | ScalarStyle::Folded) => {
                self.block_scalar(source, indent)
            }
            Value::Scalar(scalar) => scalar::write_text(&scalar.text, false),
        }
    }

    /// A block scalar's header line as written, and its lines moved to `indent`.
    fn block_scalar(&self, source: &str, indent: usize) -> String {
        let mut lines = source.trim_end_matches([' ', '\t']).lines();
        let mut out = lines.next().unwrap_or_default().trim_end().to_owned();
        let body: Vec<&str> = lines.collect();
        let written = body.iter().filter(|line| !line.trim().is_empty());
        let cut = written.map(|line| line.len() - line.trim_start_matches(' ').len()).min().unwrap_or(0);
        for line in body {
            out.push_str(self.doc.newline());
            if let Some(rest) = line.get(cut..).filter(|rest| !rest.is_empty()) {
                out.push_str(&" ".repeat(indent));
                out.push_str(rest);
            }
        }
        out
    }

    /// Ends the line with the comment written after `offset` on its line, if any.
    fn end_line(&mut self, offset: usize) {
        let rest = self.doc.text.get(offset..).unwrap_or_default();
        let line = rest.split('\n').next().unwrap_or_default().trim_end();
        let comment = line.trim_start();
        if comment.starts_with('#') && comment.len() < line.len() && !self.rules.drops(comment) {
            self.out.push_str(line);
        }
        self.out.push_str(self.doc.newline());
    }

    /// The offset after the colon that follows the key ending at `key_end`.
    fn colon_end(&self, key_end: usize) -> usize {
        let rest = self.doc.text.get(key_end..).unwrap_or_default();
        rest.find(':').map_or(key_end, |colon| key_end + colon + 1)
    }

    /// The comment lines above `entry`, when its key starts a line.
    fn above_entry(&self, entry: &Entry) -> Vec<&'d str> {
        let key = entry.key_span.start.offset;
        if self.doc.blank_before(key) { self.above(key) } else { Vec::new() }
    }

    /// The comment and blank lines directly above the line holding `offset`, trimmed; none when
    /// they reach the top of the file, where they are its header.
    fn above(&self, offset: usize) -> Vec<&'d str> {
        let mut line = self.doc.line_start(offset);
        let mut found = Vec::new();
        while line > 0 {
            let above = self.doc.line_start(line - 1);
            let text = self.doc.text.get(above..line).unwrap_or_default().trim();
            if !(text.is_empty() || text.starts_with('#')) || self.in_scalar(above) {
                found.reverse();
                return found;
            }
            found.push(text);
            line = above;
        }
        Vec::new()
    }

    /// Whether `offset` is inside a scalar written over several lines.
    fn in_scalar(&self, offset: usize) -> bool {
        self.scalars.iter().any(|span| span.start < offset && offset < span.end)
    }

    /// `lines` at `indent`: the comments the rules keep, and one blank line for each run of them.
    fn comments(&mut self, lines: &[&str], indent: usize) {
        let rules = self.rules;
        let mut blank = self.out.is_empty() || self.out.ends_with("\n\n") || self.out.ends_with("\n\r\n");
        for line in lines.iter().filter(|line| line.is_empty() || !rules.drops(line)) {
            if line.is_empty() && blank {
                continue;
            }
            blank = line.is_empty();
            if !blank {
                self.pad(indent);
                self.out.push_str(line);
            }
            self.out.push_str(self.doc.newline());
        }
    }

    /// The entries of the map at the current path in the rules' order.
    fn ordered(&self, entries: &'d [Entry]) -> Vec<&'d Entry> {
        let mut ordered: Vec<&Entry> = entries.iter().collect();
        if let Some(order) = self.rules.key_order(&self.path) {
            ordered.sort_by_key(|entry| order.iter().position(|key| *key == entry.key).unwrap_or(order.len()));
        }
        ordered
    }

    fn pad(&mut self, indent: usize) {
        self.out.push_str(&" ".repeat(indent));
    }

    /// The comment lines of the document that the rules keep, trimmed; a line inside a scalar
    /// is not one.
    fn comment_lines(&self) -> Vec<&'d str> {
        let mut found = Vec::new();
        let mut start = 0;
        for line in self.doc.text.split_inclusive('\n') {
            let text = line.trim();
            if text.starts_with('#') && !self.in_scalar(start) && !self.rules.drops(text) {
                found.push(text);
            }
            start += line.len();
        }
        found
    }
}

/// The span of every scalar in `node` written over several lines.
fn multi_line_scalars(doc: &Document, node: &Node, found: &mut Vec<Range<usize>>) {
    match &node.value {
        Value::Scalar(_) if doc.text.get(node.span.byte_range()).is_some_and(|text| text.contains('\n')) => {
            found.push(node.span.byte_range());
        }
        Value::Scalar(_) => {}
        Value::Seq(items) => items.iter().for_each(|item| multi_line_scalars(doc, item, found)),
        Value::Map(entries) => entries.iter().for_each(|entry| multi_line_scalars(doc, &entry.value, found)),
    }
}

/// Whether two trees hold the same values, whatever their order of keys and spelling.
fn same(before: &Node, after: &Node) -> bool {
    match (&before.value, &after.value) {
        (Value::Scalar(old), Value::Scalar(new)) => old.text == new.text && old.kind == new.kind,
        (Value::Seq(old), Value::Seq(new)) => old.len() == new.len() && old.iter().zip(new).all(|(old, new)| same(old, new)),
        (Value::Map(old), Value::Map(new)) => {
            let found = |entry: &Entry| new.iter().any(|other| other.key == entry.key && same(&entry.value, &other.value));
            old.len() == new.len() && old.iter().all(found)
        }
        _ => false,
    }
}
