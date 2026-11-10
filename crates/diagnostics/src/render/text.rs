use vc_model::{SourceFiles, SourceSpan};

use self::excerpt::{Excerpt, FileExcerpts, Underline};
use super::Render;
use crate::{Diagnostic, Diagnostics, DiagnosticsError, Severity};

mod escape;
mod excerpt;

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
/// Secondary labels are underlined with `-`. Excerpts are grouped by file, the primary's
/// first: each file gets one location line (`-->`, then `:::` for the others) and its excerpts
/// in line order with `...` between them. Labels whose lines overlap, touch or are one line
/// apart share an excerpt. Control characters and bidi controls in messages, labels, paths and
/// source lines are printed as visible stand-ins, so a file cannot drive the terminal. Colour
/// is off unless [`TextRenderer::color`] turns it on; the CLI decides (a terminal, and no
/// `NO_COLOR`).
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
            check_edit_files(diagnostic, files)?;
            let excerpts = excerpt::excerpts(diagnostic);
            let width = excerpt::gutter_width(&excerpts);
            Writer { files, paint: Paint(self.color), width, out: &mut out }.diagnostic(diagnostic, &excerpts)?;
        }
        Ok(out)
    }
}

/// Fails with [`DiagnosticsError::UnknownFile`] when a fix edits a file the table lacks, as
/// the JSON renderer does.
fn check_edit_files(diagnostic: &Diagnostic, files: &SourceFiles) -> Result<(), DiagnosticsError> {
    for edit in diagnostic.fixes.iter().flat_map(|fix| &fix.edits) {
        super::path_of(files, edit.file)?;
    }
    Ok(())
}

/// Writes one diagnostic; `width` is the width of the line-number gutter.
struct Writer<'a> {
    files: &'a SourceFiles,
    paint: Paint,
    width: usize,
    out: &'a mut String,
}

impl Writer<'_> {
    fn diagnostic(&mut self, diagnostic: &Diagnostic, files: &[FileExcerpts<'_>]) -> Result<(), DiagnosticsError> {
        let tone = Tone::of(diagnostic.severity);
        let head = self.paint.apply(&format!("{}[{}]", diagnostic.severity, diagnostic.code), tone);
        self.push(&format!("{head}: {}", escape::escape(&diagnostic.message)));
        for (index, file) in files.iter().enumerate() {
            self.location(if index == 0 { "-->" } else { ":::" }, &file.anchor)?;
            self.file(file, tone);
        }
        self.trailers(diagnostic);
        self.out.push('\n');
        Ok(())
    }

    fn file(&mut self, file: &FileExcerpts<'_>, tone: Tone) {
        self.gutter("", "");
        for (index, excerpt) in file.excerpts.iter().enumerate() {
            if index > 0 {
                self.push(&self.paint.apply("...", Tone::Gutter));
            }
            self.excerpt(excerpt, tone);
        }
    }

    fn location(&mut self, arrow: &str, span: &SourceSpan) -> Result<(), DiagnosticsError> {
        let path = super::path_of(self.files, span.file)?;
        let arrow = self.paint.apply(arrow, Tone::Gutter);
        let pad = " ".repeat(self.width);
        self.push(&format!("{pad}{arrow} {}:{}:{}", escape::escape(path.as_str()), span.start.line, span.start.column));
        Ok(())
    }

    fn excerpt(&mut self, excerpt: &Excerpt<'_>, tone: Tone) {
        for line in excerpt.shown_lines() {
            match line {
                Some(number) => self.source_line(excerpt, number, tone),
                None => self.push(&self.paint.apply("...", Tone::Gutter)),
            }
        }
    }

    fn source_line(&mut self, excerpt: &Excerpt<'_>, number: u32, tone: Tone) {
        let Some(text) = self.files.line(excerpt.file, number) else { return };
        self.gutter(&number.to_string(), &escape::escape(text));
        for mark in excerpt.marks_on(number) {
            let tone = if mark.primary { tone } else { Tone::Secondary };
            self.underline(&mark.underline(number, text), tone);
        }
    }

    fn underline(&mut self, underline: &Underline<'_>, tone: Tone) {
        let carets = String::from(tone.caret()).repeat(underline.width);
        let mut marks = format!("{}{}", underline.pad, self.paint.apply(&carets, tone));
        if let Some(text) = underline.text {
            marks = format!("{marks} {}", self.paint.apply(&escape::escape(text), tone));
        }
        self.gutter("", &marks);
    }

    fn gutter(&mut self, number: &str, rest: &str) {
        let bar = self.paint.apply(&format!("{number:>width$} |", width = self.width), Tone::Gutter);
        if rest.is_empty() { self.push(&bar) } else { self.push(&format!("{bar} {rest}")) }
    }

    fn trailers(&mut self, diagnostic: &Diagnostic) {
        let notes = diagnostic.notes.iter().map(|note| ("note", note.as_str(), ""));
        let help = diagnostic.help.iter().map(|help| ("help", help.as_str(), ""));
        let fixes =
            diagnostic.fixes.iter().map(|fix| ("fix", fix.title.as_str(), if fix.certain { " (certain)" } else { "" }));
        let pad = " ".repeat(self.width + 1);
        let equals = self.paint.apply("=", Tone::Gutter);
        for (kind, text, suffix) in notes.chain(help).chain(fixes) {
            self.push(&format!("{pad}{equals} {kind}: {}{suffix}", escape::escape(text)));
        }
    }

    fn push(&mut self, line: &str) {
        self.out.push_str(line);
        self.out.push('\n');
    }
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
