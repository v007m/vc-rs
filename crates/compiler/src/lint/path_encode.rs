//! V4007: a `path` value whose last pipe is `urlEncode`. A run encodes each value in a `path`
//! itself (D278) and leaves such a value as it is, so the pipe does nothing but can be removed.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::{PipeCall, Pos, SourceSpan};

use super::{Lint, LintCx};

/// Reports each `| urlEncode` that ends a value in an api's `path`.
pub struct PathEncode;

impl Lint for PathEncode {
    fn code(&self) -> Code {
        Code::V4007
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let references = cx.program.apis.values().flat_map(|api| api.path.references());
        let pipes = references.filter_map(|reference| reference.pipes.last().filter(|pipe| pipe.name == "urlEncode"));
        pipes.map(|pipe| self.redundant(cx, pipe)).collect()
    }
}

impl PathEncode {
    /// V4007 at the pipe, with the certain fix that removes it and its `|`.
    fn redundant(&self, cx: &LintCx<'_, '_>, pipe: &PipeCall) -> Diagnostic {
        let message = "`urlEncode` is not needed in a `path`: a run encodes each value there";
        let diagnostic = Diagnostic::new(self.code(), pipe.span, message)
            .label("not needed")
            .note("a run sends a value that ends in `urlEncode` as the pipe left it, so it is not encoded twice")
            .help("remove `| urlEncode`");
        match removal(cx, pipe) {
            Some(span) => diagnostic.fix(Fix::certain("Remove `| urlEncode`", vec![TextEdit::replace(span, "")])),
            None => diagnostic,
        }
    }
}

/// The pipe with the `|` and the spaces before it, when the source holds just that there.
fn removal(cx: &LintCx<'_, '_>, pipe: &PipeCall) -> Option<SourceSpan> {
    let text = cx.project.sources.iter().find(|source| source.file == pipe.span.file)?.text;
    let before = text.get(..pipe.span.start.offset)?;
    let bar = before.trim_end().strip_suffix('|')?.trim_end().len();
    let taken = pipe.span.start.offset.checked_sub(bar)?;
    let column = pipe.span.start.column.checked_sub(u32::try_from(taken).ok()?)?;
    let start = Pos::new(pipe.span.start.line, column, bar);
    let one_line = !text.get(bar..pipe.span.start.offset)?.contains('\n');
    one_line.then(|| SourceSpan::new(pipe.span.file, start, pipe.span.end))
}
