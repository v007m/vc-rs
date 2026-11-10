//! Example columns: `${example.x}` in a test whose `examples` rows have no `x` (V2103). The rows
//! are written inline, or come from a dataset or `$file` the compile was given (D173); rows from
//! any other template are only known at run time, so they are not checked.

use std::collections::{BTreeSet, HashSet};

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{FileKind, Namespace, Reference, Segment, SourceSpan};
use vc_parser::{Node, Value};

use super::data::Data;
use super::project;
use super::project::{Project, Source};
use super::uses;
use crate::suggest;

/// A test's inline rows: where they are written, and every key any row has.
struct Rows<'a> {
    span: SourceSpan,
    columns: BTreeSet<&'a str>,
}

/// Checks every `${example.…}` of every test whose rows are known.
pub fn check(project: &Project<'_>, data: &Data<'_>) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    for source in project.sources.iter().filter(|source| source.kind == FileKind::Test) {
        for test in project::list(source.root, "tests") {
            let Some(examples) = test.get("examples") else { continue };
            let Some(rows) = rows(examples, data.rows_at(source, examples).unwrap_or(examples)) else { continue };
            diagnostics.extend(unknown(source, test, &rows));
        }
    }
    diagnostics
}

/// The rows, when `rows` is a list of maps; `examples` is where they are named.
fn rows<'a>(examples: &Node, rows: &'a Node) -> Option<Rows<'a>> {
    let Value::Seq(rows) = &rows.value else { return None };
    if !rows.iter().all(|row| matches!(row.value, Value::Map(_))) {
        return None;
    }
    let columns = rows.iter().flat_map(|row| row.entries().iter().map(|entry| entry.key.as_str())).collect();
    Some(Rows { span: examples.span, columns })
}

/// V2103 for each column `test` reads that no row has; the rows themselves are not read.
fn unknown(source: &Source<'_>, test: &Node, rows: &Rows<'_>) -> Vec<Diagnostic> {
    let holes = [rows.span];
    let mut found = Vec::new();
    for piece in source.pieces(test.span, &holes) {
        uses::piece_refs(piece, &mut found);
    }
    let mut seen = HashSet::new();
    let examples = found.into_iter().filter(|reference| reference.ns == Namespace::Example);
    examples
        .filter(|reference| seen.insert(reference.span))
        .filter_map(|reference| column(source, reference, rows))
        .collect()
}

fn column(source: &Source<'_>, reference: &Reference, rows: &Rows<'_>) -> Option<Diagnostic> {
    let Some(Segment::Key(name)) = reference.path.segments.first() else { return None };
    if rows.columns.contains(name.as_str()) {
        return None;
    }
    let message = format!("${{example.{name}}} names no column of this test's examples");
    let diagnostic = Diagnostic::new(Code::V2103, reference.span, message)
        .label(format!("no row has `{name}`"))
        .secondary(rows.span, "the rows");
    let known: Vec<&str> = rows.columns.iter().copied().collect();
    let diagnostic = diagnostic.help(format!("the columns are {}", suggest::quoted(&known)));
    Some(uses::did_you_mean(diagnostic, (source, reference, name), &known))
}
