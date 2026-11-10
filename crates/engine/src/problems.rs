use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_interfaces::SourceError;
use vc_model::{Pos, SourceFile, SourceFiles, SourcePath, SourceSpan};

/// What a diagnostic names when a problem has no usable path.
const PROJECT: &str = "<project>";

/// Reports each file discovery could not read as a V1012 error at the start of its path.
///
/// The file goes into `files` with no text, so the diagnostic has a file to point at; a name
/// a [`SourcePath`] refuses is shown with the bad characters replaced.
pub fn add_problems(problems: &[SourceError], diagnostics: &mut Diagnostics, files: &mut SourceFiles) {
    for problem in problems {
        let path = problem.path().and_then(|path| SourcePath::lossy(path).ok());
        let Some(path) = path.or_else(|| SourcePath::new(PROJECT).ok()) else { continue };
        let Ok(id) = files.push(SourceFile { path, text: String::new(), hash: vc_interfaces::content_hash(&[]) })
        else {
            break;
        };
        let start = Pos::new(1, 1, 0);
        let mut diagnostic = Diagnostic::new(Code::V1012, SourceSpan::new(id, start, start), problem.to_string());
        if let Some(help) = help(problem) {
            diagnostic = diagnostic.help(help);
        }
        diagnostics.push(diagnostic);
    }
}

const fn help(problem: &SourceError) -> Option<&'static str> {
    match problem {
        SourceError::Missing { .. } => Some("check the path, or create the file"),
        SourceError::Read { .. } => Some("check that the file is readable"),
        SourceError::NotUtf8 { .. } => Some("save the file as UTF-8"),
        SourceError::InvalidPath { .. } => {
            Some("rename the file: a name must be UTF-8 and hold no `:`, control or bidi characters")
        }
        SourceError::OutsideRoot { .. } => Some("copy the file into the project instead of linking to it"),
        _ => None,
    }
}
