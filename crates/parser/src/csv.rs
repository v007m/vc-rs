//! The CSV reader for datasets (D173): a header row, then one row per record, read into the same
//! tree as YAML and JSON. Every cell is text (a double-quoted scalar), and every cell of a record
//! spans the whole record.

use std::collections::BTreeSet;

use ::csv::{Error, ErrorKind, Position, Reader, ReaderBuilder, StringRecord};
use vc_diagnostics::Diagnostics;
use vc_model::{Pos, SourceFileId, SourceSpan};

use crate::{Entry, Node, Scalar, ScalarStyle, Value, problems};

/// Turns byte offsets, read in order, into positions.
struct Cursor<'t> {
    file: SourceFileId,
    text: &'t str,
    at: Pos,
}

impl Cursor<'_> {
    /// The position of `offset`, at or after the last one asked for.
    fn pos(&mut self, offset: usize) -> Pos {
        let offset = offset.min(self.text.len());
        let skipped = self.text.get(self.at.offset..offset).unwrap_or_default();
        for c in skipped.chars() {
            self.at = if c == '\n' {
                Pos::new(self.at.line.saturating_add(1), 1, self.at.offset.saturating_add(1))
            } else {
                let offset = self.at.offset.saturating_add(c.len_utf8());
                Pos::new(self.at.line, self.at.column.saturating_add(1), offset)
            };
        }
        self.at
    }

    /// The span from `start` to `end`, without the line break that ends a record.
    fn span(&mut self, start: usize, end: usize) -> SourceSpan {
        let line = self.text.get(start..end).unwrap_or_default();
        let end = start.saturating_add(line.trim_end_matches(['\r', '\n']).len());
        SourceSpan::new(self.file, self.pos(start), self.pos(end))
    }

    /// The span of the record that starts at `start` and ends where `reader` stopped.
    fn record(&mut self, start: Option<&Position>, reader: &Reader<&[u8]>) -> SourceSpan {
        let start = start.map_or(0, Position::byte);
        let end = usize::try_from(reader.position().byte()).unwrap_or(self.text.len());
        self.span(usize::try_from(start).unwrap_or(self.text.len()), end)
    }
}

/// The header: each column the first time it is named, with its index, and its span.
struct Header {
    columns: Vec<(usize, String)>,
    span: SourceSpan,
}

/// Reads `text` as a CSV dataset: a list of maps from column name to text. A record with
/// another number of cells, or text that is not UTF-8, stops the read (V1001); a column
/// named twice is V1006, and its first cells are kept.
pub fn read(file: SourceFileId, text: &str) -> (Option<Node>, Diagnostics) {
    let mut cursor = Cursor { file, text, at: Pos::START };
    let mut reader = ReaderBuilder::new().from_reader(text.as_bytes());
    let names = match reader.headers() {
        Ok(names) => names.clone(),
        Err(error) => return (None, broken(&mut cursor, &error)),
    };
    let span = cursor.record(None, &reader);
    let (header, mut diagnostics) = header(&names, span);
    match rows(&mut reader, &mut cursor, &header) {
        Ok(rows) => {
            let span = SourceSpan::new(file, Pos::START, cursor.pos(text.len()));
            (Some(Node { value: Value::Seq(rows), span }), diagnostics)
        }
        Err(problem) => {
            diagnostics.extend(broken(&mut cursor, &problem));
            (None, diagnostics)
        }
    }
}

/// The header's columns, and V1006 for each one named again.
fn header(names: &StringRecord, span: SourceSpan) -> (Header, Diagnostics) {
    let mut seen = BTreeSet::new();
    let mut diagnostics = Diagnostics::new();
    let mut columns = Vec::new();
    for (index, name) in names.iter().enumerate() {
        if seen.insert(name) {
            columns.push((index, name.to_owned()));
        } else {
            diagnostics.push(problems::duplicate_column(span, name));
        }
    }
    (Header { columns, span }, diagnostics)
}

/// Every record after the header, as a row.
fn rows(reader: &mut Reader<&[u8]>, cursor: &mut Cursor<'_>, header: &Header) -> Result<Vec<Node>, Error> {
    let mut rows = Vec::new();
    let mut record = StringRecord::new();
    while reader.read_record(&mut record)? {
        let span = cursor.record(record.position(), reader);
        rows.push(row(header, &record, span));
    }
    Ok(rows)
}

/// One record as a map from each column to its cell.
fn row(header: &Header, record: &StringRecord, span: SourceSpan) -> Node {
    let cells = header.columns.iter().map(|(index, column)| Entry {
        key: column.clone(),
        key_span: header.span,
        value: Node {
            value: Value::Scalar(Scalar::new(record.get(*index).unwrap_or_default(), ScalarStyle::DoubleQuoted)),
            span,
        },
    });
    Node { value: Value::Map(cells.collect()), span }
}

/// The reader's error as V1001 on the line it stopped in.
fn broken(cursor: &mut Cursor<'_>, error: &Error) -> Diagnostics {
    let start = error.position().map_or(0, Position::byte);
    let start = usize::try_from(start).unwrap_or(usize::MAX).min(cursor.text.len());
    let line = cursor.text.get(start..).and_then(|rest| rest.find('\n'));
    let at = cursor.span(start, line.map_or(cursor.text.len(), |end| start.saturating_add(end)));
    let message = match error.kind() {
        ErrorKind::UnequalLengths { expected_len, len, .. } => {
            format!("this row has {len} cells, but the header has {expected_len}")
        }
        _ => error.to_string(),
    };
    let mut diagnostics = Diagnostics::new();
    diagnostics.push(problems::csv_syntax(at, &message));
    diagnostics
}
