use serde::Serialize;
use vc_model::SourceSpan;

use crate::{Code, Fix, Severity};

/// A place in a source file with a short text shown under it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Label {
    /// The place.
    pub span: SourceSpan,
    /// The text under the caret; may be empty.
    pub text: String,
}

impl Label {
    /// A label with `text` at `span`.
    #[must_use]
    pub fn new(span: SourceSpan, text: impl Into<String>) -> Self {
        Self { span, text: text.into() }
    }
}

/// One problem found in the user's files or during a run, with what to do about it.
///
/// Built with [`Diagnostic::new`] and the chained methods; the severity comes from the code.
///
/// The serde form is a base for reporters, not the `vc check --json` output: spans hold a
/// `SourceFileId` index, which the JSON reporter replaces with the file path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Diagnostic {
    /// The stable code: V2003.
    pub code: Code,
    /// How serious it is.
    pub severity: Severity,
    /// What is wrong: `unknown api "orders#plcae"`.
    pub message: String,
    /// The main place, with a short label under the caret.
    pub primary: Label,
    /// Other places involved, such as the earlier duplicate.
    pub secondary: Vec<Label>,
    /// Background shown as `= note: …`.
    pub notes: Vec<String>,
    /// Advice shown as `= help: …`.
    pub help: Vec<String>,
    /// Changes that resolve the problem.
    pub fixes: Vec<Fix>,
    /// Places whose text excerpts never print, shown as `•••`: a literal token (D314).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub masked: Vec<SourceSpan>,
}

impl Diagnostic {
    /// A diagnostic for `code` at `span`, with the code's default severity.
    #[must_use]
    pub fn new(code: Code, span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: code.default_severity(),
            message: message.into(),
            primary: Label::new(span, ""),
            secondary: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
            fixes: Vec::new(),
            masked: Vec::new(),
        }
    }

    /// Sets the text under the primary span.
    #[must_use]
    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.primary.text = text.into();
        self
    }

    /// Adds another place involved in the problem.
    #[must_use]
    pub fn secondary(mut self, span: SourceSpan, text: impl Into<String>) -> Self {
        self.secondary.push(Label::new(span, text));
        self
    }

    /// Adds a `note:` line.
    #[must_use]
    pub fn note(mut self, text: impl Into<String>) -> Self {
        self.notes.push(text.into());
        self
    }

    /// Adds a `help:` line.
    #[must_use]
    pub fn help(mut self, text: impl Into<String>) -> Self {
        self.help.push(text.into());
        self
    }

    /// Adds a fix.
    #[must_use]
    pub fn fix(mut self, fix: Fix) -> Self {
        self.fixes.push(fix);
        self
    }

    /// Keeps the text at `span` out of every excerpt: the text renderer prints `•••` there.
    #[must_use]
    pub fn masked(mut self, span: SourceSpan) -> Self {
        self.masked.push(span);
        self
    }

    /// Whether this diagnostic is an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vc_model::{Pos, SourceFileId, SourceSpan};

    use super::Diagnostic;
    use crate::{Code, Fix, Severity, TextEdit};

    fn span(line: u32) -> SourceSpan {
        SourceSpan::new(SourceFileId::new(0), Pos::new(line, 3, 10), Pos::new(line, 7, 14))
    }

    #[test]
    fn new_takes_severity_from_the_code() {
        assert_eq!(Diagnostic::new(Code::V2003, span(1), "unknown").severity, Severity::Error);
        let warning = Diagnostic::new(Code::V3104, span(1), "expired");
        assert_eq!(warning.severity, Severity::Warning);
        assert!(!warning.is_error());
    }

    #[test]
    fn builder_collects_every_part() {
        let fix = Fix::certain("Add the unit", vec![TextEdit::replace(span(2), "500ms")]);
        let diagnostic = Diagnostic::new(Code::V2301, span(2), "invalid duration")
            .label("no unit")
            .secondary(span(1), "default set here")
            .note("durations need a unit")
            .help("write 500ms")
            .fix(fix.clone());
        assert_eq!(diagnostic.primary.text, "no unit");
        assert_eq!(diagnostic.secondary[0].text, "default set here");
        assert_eq!((diagnostic.notes.len(), diagnostic.help.len()), (1, 1));
        assert_eq!(diagnostic.fixes, [fix]);
    }

    #[test]
    fn serializes_with_camel_case_keys_and_text_codes() {
        let diagnostic = Diagnostic::new(Code::V1004, span(4), "unknown key `tset`")
            .fix(Fix::certain("Rename to test", vec![TextEdit::replace(span(4), "test")]));
        let at = json!({"file": 0, "start": {"line": 4, "column": 3, "offset": 10}, "end": {"line": 4, "column": 7, "offset": 14}});
        let expected = json!({
            "code": "V1004", "severity": "error", "message": "unknown key `tset`",
            "primary": {"span": at, "text": ""}, "secondary": [], "notes": [], "help": [],
            "fixes": [{"title": "Rename to test", "certain": true,
                       "edits": [{"file": 0, "range": {"start": 10, "end": 14}, "newText": "test"}]}],
        });
        assert_eq!(serde_json::to_value(&diagnostic).unwrap(), expected);
    }
}
