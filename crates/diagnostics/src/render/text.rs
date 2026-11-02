use vc_model::{SourceFiles, SourceSpan};

use super::{Render, path_of};
use crate::{Diagnostic, Diagnostics, DiagnosticsError, Label, Severity};

/// Spans covering more lines than this show their first two lines, `...`, and their last.
const MAX_FULL_LINES: u32 = 4;

/// Renders diagnostics for a terminal, in the style of rustc:
///
/// ```text
/// error[V2003]: unknown api "orders#plcae"
///   --> services/orders/checkout.test.yaml:41:15
///    |
/// 41 |         call: orders#plcae
///    |               ^^^^^^^^^^^^ not defined
///    = help: did you mean orders#place?
///    = fix: Replace with orders#place (certain)
/// ```
///
/// Secondary labels are underlined with `-`, and those in another file get a `:::` line with
/// their location. Colour is off unless [`TextRenderer::color`] turns it on; the CLI decides
/// (a terminal, and no `NO_COLOR`).
#[derive(Clone, Copy, Debug, Default)]
pub struct TextRenderer {
    color: bool,
}

impl TextRenderer {
    /// The same renderer with ANSI colour turned on or off.
    #[must_use]
    pub const fn color(self, on: bool) -> Self {
        Self { color: on }
    }
}

impl Render for TextRenderer {
    fn render(&self, diagnostics: &Diagnostics, files: &SourceFiles) -> Result<String, DiagnosticsError> {
        let mut out = String::new();
        for diagnostic in diagnostics.iter() {
            let width = gutter_width(diagnostic);
            Writer { files, paint: Paint(self.color), width, out: &mut out }.diagnostic(diagnostic)?;
        }
        Ok(out)
    }
}

/// Writes one diagnostic; `width` is the width of the line-number gutter.
struct Writer<'a> {
    files: &'a SourceFiles,
    paint: Paint,
    width: usize,
    out: &'a mut String,
}

impl Writer<'_> {
    fn diagnostic(&mut self, diagnostic: &Diagnostic) -> Result<(), DiagnosticsError> {
        let tone = Tone::of(diagnostic.severity);
        let head = self.paint.apply(&format!("{}[{}]", diagnostic.severity, diagnostic.code), tone);
        self.push(&format!("{head}: {}", diagnostic.message));
        self.location("-->", &diagnostic.primary.span)?;
        self.snippet(&diagnostic.primary, tone);
        for label in &diagnostic.secondary {
            if label.span.file != diagnostic.primary.span.file {
                self.location(":::", &label.span)?;
            }
            self.snippet(label, Tone::Secondary);
        }
        self.trailers(diagnostic);
        self.out.push('\n');
        Ok(())
    }

    fn location(&mut self, arrow: &str, span: &SourceSpan) -> Result<(), DiagnosticsError> {
        let path = path_of(self.files, span.file)?;
        let arrow = self.paint.apply(arrow, Tone::Gutter);
        let pad = " ".repeat(self.width);
        self.push(&format!("{pad}{arrow} {path}:{}:{}", span.start.line, span.start.column));
        Ok(())
    }

    fn snippet(&mut self, label: &Label, tone: Tone) {
        self.gutter("", "");
        for line in shown_lines(&label.span) {
            match line {
                Some(number) => self.source_line(label, number, tone),
                None => self.push(&self.paint.apply("...", Tone::Gutter)),
            }
        }
    }

    fn source_line(&mut self, label: &Label, number: u32, tone: Tone) {
        let Some(text) = self.files.line(label.span.file, number) else { return };
        self.gutter(&number.to_string(), text);
        let (from, to) = underline_columns(&label.span, number, text);
        let pad: String = text.chars().take(from - 1).map(|c| if c == '\t' { '\t' } else { ' ' }).collect();
        let carets = String::from(tone.caret()).repeat(to.saturating_sub(from).max(1));
        let mut marks = format!("{pad}{}", self.paint.apply(&carets, tone));
        if number == label.span.end.line && !label.text.is_empty() {
            marks = format!("{marks} {}", self.paint.apply(&label.text, tone));
        }
        self.gutter("", &marks);
    }

    fn gutter(&mut self, number: &str, rest: &str) {
        let bar = self.paint.apply(&format!("{number:>width$} |", width = self.width), Tone::Gutter);
        if rest.is_empty() { self.push(&bar) } else { self.push(&format!("{bar} {rest}")) }
    }

    fn trailers(&mut self, diagnostic: &Diagnostic) {
        let notes = diagnostic.notes.iter().map(|note| ("note", note.clone()));
        let help = diagnostic.help.iter().map(|help| ("help", help.clone()));
        let fixes = diagnostic
            .fixes
            .iter()
            .map(|fix| ("fix", if fix.certain { format!("{} (certain)", fix.title) } else { fix.title.clone() }));
        let pad = " ".repeat(self.width + 1);
        let equals = self.paint.apply("=", Tone::Gutter);
        for (kind, text) in notes.chain(help).chain(fixes) {
            self.push(&format!("{pad}{equals} {kind}: {text}"));
        }
    }

    fn push(&mut self, line: &str) {
        self.out.push_str(line);
        self.out.push('\n');
    }
}

/// The width of the widest line number any label of `diagnostic` shows.
fn gutter_width(diagnostic: &Diagnostic) -> usize {
    let labels = std::iter::once(&diagnostic.primary).chain(&diagnostic.secondary);
    labels.map(|label| label.span.end.line).max().unwrap_or(1).to_string().len()
}

/// The lines to show for `span`; `None` stands for the elided middle of a long span.
fn shown_lines(span: &SourceSpan) -> Vec<Option<u32>> {
    let (first, last) = (span.start.line, span.end.line.max(span.start.line));
    if last - first < MAX_FULL_LINES {
        (first..=last).map(Some).collect()
    } else {
        vec![Some(first), Some(first + 1), None, Some(last)]
    }
}

/// The 1-based columns to underline on `line`, from the first up to (not including) the second.
///
/// The span's first line is underlined from its start column, its last line up to its end
/// column, and lines between from their first non-blank character to their end.
fn underline_columns(span: &SourceSpan, line: u32, text: &str) -> (usize, usize) {
    let past_end = text.chars().count() + 1;
    let first_visible = text.chars().take_while(|c| c.is_whitespace()).count() + 1;
    let from = if line == span.start.line { column(span.start.column) } else { first_visible };
    let to = if line == span.end.line { column(span.end.column) } else { past_end };
    (from.clamp(1, past_end), to.clamp(1, past_end))
}

fn column(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// The colour a piece of output is drawn in.
#[derive(Clone, Copy)]
enum Tone {
    Error,
    Warning,
    Info,
    Secondary,
    Gutter,
}

impl Tone {
    const fn of(severity: Severity) -> Self {
        match severity {
            Severity::Error => Self::Error,
            Severity::Warning => Self::Warning,
            Severity::Info => Self::Info,
        }
    }

    const fn ansi(self) -> &'static str {
        match self {
            Self::Error => "1;31",
            Self::Warning => "1;33",
            Self::Info => "1;36",
            Self::Secondary | Self::Gutter => "1;34",
        }
    }

    const fn caret(self) -> char {
        if matches!(self, Self::Secondary) { '-' } else { '^' }
    }
}

/// Wraps text in ANSI colour codes when colour is on.
#[derive(Clone, Copy)]
struct Paint(bool);

impl Paint {
    fn apply(self, text: &str, tone: Tone) -> String {
        if self.0 { format!("\x1b[{}m{text}\x1b[0m", tone.ansi()) } else { text.to_owned() }
    }
}
