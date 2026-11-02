use std::ops::Range;

use serde::Serialize;
use vc_model::{SourceFiles, SourceSpan};

use super::{Render, path_of};
use crate::{Code, Diagnostic, Diagnostics, DiagnosticsError, Fix, Label, Severity, Summary, TextEdit};

/// Renders diagnostics as the stable JSON of `vc check --json`, for scripts, editors and agents.
///
/// Spans name the file by path, with 1-based lines and character columns; fix edits keep
/// their byte ranges. The shape is documented in the LLD, "Diagnostics and fixes".
#[derive(Clone, Copy, Debug, Default)]
pub struct JsonRenderer;

impl Render for JsonRenderer {
    fn render(&self, diagnostics: &Diagnostics, files: &SourceFiles) -> Result<String, DiagnosticsError> {
        let report = Report {
            summary: diagnostics.summary(),
            diagnostics: diagnostics.iter().map(|d| JsonDiagnostic::new(d, files)).collect::<Result<_, _>>()?,
        };
        serde_json::to_string_pretty(&report).map_err(|error| DiagnosticsError::Json(error.to_string()))
    }
}

#[derive(Serialize)]
struct Report<'a> {
    summary: Summary,
    diagnostics: Vec<JsonDiagnostic<'a>>,
}

#[derive(Serialize)]
struct JsonDiagnostic<'a> {
    code: Code,
    severity: Severity,
    message: &'a str,
    primary: JsonLabel<'a>,
    secondary: Vec<JsonLabel<'a>>,
    notes: &'a [String],
    help: &'a [String],
    fixes: Vec<JsonFix<'a>>,
}

impl<'a> JsonDiagnostic<'a> {
    fn new(diagnostic: &'a Diagnostic, files: &'a SourceFiles) -> Result<Self, DiagnosticsError> {
        Ok(Self {
            code: diagnostic.code,
            severity: diagnostic.severity,
            message: &diagnostic.message,
            primary: JsonLabel::new(&diagnostic.primary, files)?,
            secondary: diagnostic.secondary.iter().map(|l| JsonLabel::new(l, files)).collect::<Result<_, _>>()?,
            notes: &diagnostic.notes,
            help: &diagnostic.help,
            fixes: diagnostic.fixes.iter().map(|f| JsonFix::new(f, files)).collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Serialize)]
struct JsonLabel<'a> {
    span: Location<'a>,
    text: &'a str,
}

impl<'a> JsonLabel<'a> {
    fn new(label: &'a Label, files: &'a SourceFiles) -> Result<Self, DiagnosticsError> {
        Ok(Self { span: Location::new(&label.span, files)?, text: &label.text })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Location<'a> {
    path: &'a str,
    line: u32,
    column: u32,
    end_line: u32,
    end_column: u32,
}

impl<'a> Location<'a> {
    fn new(span: &SourceSpan, files: &'a SourceFiles) -> Result<Self, DiagnosticsError> {
        Ok(Self {
            path: path_of(files, span.file)?.as_str(),
            line: span.start.line,
            column: span.start.column,
            end_line: span.end.line,
            end_column: span.end.column,
        })
    }
}

#[derive(Serialize)]
struct JsonFix<'a> {
    title: &'a str,
    certain: bool,
    edits: Vec<JsonEdit<'a>>,
}

impl<'a> JsonFix<'a> {
    fn new(fix: &'a Fix, files: &'a SourceFiles) -> Result<Self, DiagnosticsError> {
        let edits = fix.edits.iter().map(|e| JsonEdit::new(e, files)).collect::<Result<_, _>>()?;
        Ok(Self { title: &fix.title, certain: fix.certain, edits })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonEdit<'a> {
    path: &'a str,
    range: Range<usize>,
    new_text: &'a str,
}

impl<'a> JsonEdit<'a> {
    fn new(edit: &'a TextEdit, files: &'a SourceFiles) -> Result<Self, DiagnosticsError> {
        Ok(Self { path: path_of(files, edit.file)?.as_str(), range: edit.range.clone(), new_text: &edit.new_text })
    }
}
