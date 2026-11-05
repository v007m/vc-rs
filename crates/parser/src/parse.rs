use vc_diagnostics::Diagnostics;
use vc_model::SourceFileId;

use crate::{Node, Syntax, csv, json, yaml};

/// Parses one YAML, JSON or CSV file into a [`Node`] tree, collecting every problem it can find.
///
/// A CSV file (a dataset, D173) gives a list with one map per row after the header, from column
/// name to the cell's text; a short or long row, or text that is not UTF-8, is V1001, and a
/// column named twice is V1006. Every cell of a row spans the whole row.
///
/// Both syntaxes give the same tree: maps keep key order and key spans, scalars keep their
/// text, style and YAML 1.2 core kind. The root is `None` when the reader stops on an error
/// (V1001; for YAML also V1002 for a structure problem and V1007 for an alias to a missing
/// anchor) and when nesting goes deeper than 128 levels (V1011); an empty or blank file gives
/// a null scalar. Problems the tree can survive (V1006 duplicate key; for YAML also V1007
/// alias, V1008 tag, V1009 non-text key, V1010 second document) are reported and the tree is
/// still returned, so later stages can report more. YAML node spans cover the value itself,
/// without any `&anchor` or `!tag` written before it. JSON is read strictly (RFC 8259, D148):
/// no trailing commas or comments, and `\u` escapes with full surrogate pairs.
///
/// ```
/// use vc_model::SourceFileId;
/// use vc_parser::{Node, Syntax};
///
/// let (root, diagnostics) = vc_parser::parse(SourceFileId::new(0), Syntax::Yaml, "id: getsUser\nretries: 2\n");
/// let root = root.unwrap();
/// assert!(diagnostics.is_empty());
/// assert_eq!(root.get("id").and_then(Node::as_str), Some("getsUser"));
/// assert_eq!(root.entries()[1].key_span.start.line, 2);
///
/// let (json, _) = vc_parser::parse(SourceFileId::new(1), Syntax::Json, r#"{"id": "getsUser"}"#);
/// assert_eq!(json.unwrap().get("id").and_then(Node::as_str), Some("getsUser"));
/// ```
#[must_use]
pub fn parse(file: SourceFileId, syntax: Syntax, text: &str) -> (Option<Node>, Diagnostics) {
    match syntax {
        Syntax::Yaml => yaml::read(file, text),
        Syntax::Json => json::read(file, text),
        Syntax::Csv => csv::read(file, text),
    }
}
