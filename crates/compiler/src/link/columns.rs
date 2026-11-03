//! Example columns: `${example.x}` in a test whose inline `examples` rows have no `x` (V2103).
//! Rows from a `$file` or a template are only known at run time, so they are not checked.

use std::collections::{BTreeSet, HashSet};

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{FileKind, Namespace, Reference, Segment, SourceSpan};
use vc_parser::{Node, Value};

use super::project::{Project, Source, list};
use super::uses::{did_you_mean, piece_refs};
use crate::suggest;

/// A test's inline rows: where they are written, and every key any row has.
struct Rows<'a> {
    span: SourceSpan,
    columns: BTreeSet<&'a str>,
}

/// Checks every `${example.…}` of every test with inline rows.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    for source in project.sources.iter().filter(|source| source.kind == FileKind::Test) {
        for test in list(source.root, "tests") {
            let Some(rows) = test.get("examples").and_then(rows) else { continue };
            diagnostics.extend(unknown(source, test, &rows));
        }
    }
    diagnostics
}

/// The rows, when `examples` is a list of maps.
fn rows(examples: &Node) -> Option<Rows<'_>> {
    let Value::Seq(rows) = &examples.value else { return None };
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
        piece_refs(piece, &mut found);
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
    Some(did_you_mean(diagnostic, (source, reference, name), &known))
}
