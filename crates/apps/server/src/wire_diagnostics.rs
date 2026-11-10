//! A check's diagnostics in their wire form, as `vc check --json` reports them: each span names
//! its file by path, and each fix's edits by path and byte offsets.

use vc_contracts::engine::v1 as wire;
use vc_contracts::results::v1::Span;
use vc_engine::{CheckReport, Diagnostic, Fix, Label, Severity, SourceFiles, SourceSpan};

/// Every diagnostic of `report`, in its order.
pub fn diagnostics(report: &CheckReport) -> Vec<wire::Diagnostic> {
    report.diagnostics.iter().map(|found| diagnostic(&report.files, found)).collect()
}

fn diagnostic(files: &SourceFiles, found: &Diagnostic) -> wire::Diagnostic {
    wire::Diagnostic {
        code: found.code.to_string(),
        severity: severity(found.severity).into(),
        message: found.message.clone(),
        primary: Some(label(files, &found.primary)),
        secondary: found.secondary.iter().map(|other| label(files, other)).collect(),
        notes: found.notes.clone(),
        help: found.help.clone(),
        fixes: found.fixes.iter().map(|fix| self::fix(files, fix)).collect(),
    }
}

const fn severity(severity: Severity) -> wire::Severity {
    match severity {
        Severity::Info => wire::Severity::Info,
        Severity::Warning => wire::Severity::Warning,
        Severity::Error => wire::Severity::Error,
    }
}

fn label(files: &SourceFiles, label: &Label) -> wire::Label {
    wire::Label { span: Some(span(files, label.span)), text: label.text.clone() }
}

fn fix(files: &SourceFiles, fix: &Fix) -> wire::Fix {
    let edits = fix.edits.iter().map(|edit| wire::TextEdit {
        file: path(files, edit.file),
        start: offset(edit.range.start),
        end: offset(edit.range.end),
        new_text: edit.new_text.clone(),
    });
    wire::Fix { title: fix.title.clone(), certain: fix.certain, edits: edits.collect() }
}

/// `span` with its file's path, relative to the project root.
pub fn span(files: &SourceFiles, span: SourceSpan) -> Span {
    Span {
        file: path(files, span.file),
        start_line: span.start.line,
        start_column: span.start.column,
        end_line: span.end.line,
        end_column: span.end.column,
    }
}

fn path(files: &SourceFiles, id: vc_engine::SourceFileId) -> String {
    files.get(id).map(|file| file.path.as_str().to_owned()).unwrap_or_default()
}

/// A byte offset; one past `u32::MAX` saturates, as no project file is that long.
fn offset(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[path = "wire_diagnostics_tests.rs"]
mod tests;
