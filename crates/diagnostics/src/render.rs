use vc_model::{SourceFileId, SourceFiles, SourcePath};

use crate::{Diagnostics, DiagnosticsError};

mod json;
mod text;

pub use json::JsonRenderer;
pub use text::TextRenderer;

/// Turns diagnostics into output for people or tools, in the order they are given.
///
/// Sort them first with [`Diagnostics::sort`] for display order.
pub trait Render {
    /// Renders `diagnostics`, resolving their spans against `files`.
    ///
    /// # Errors
    ///
    /// [`DiagnosticsError::UnknownFile`] when a span or edit names a file missing from `files`;
    /// [`DiagnosticsError::Json`] when JSON output cannot be written.
    fn render(&self, diagnostics: &Diagnostics, files: &SourceFiles) -> Result<String, DiagnosticsError>;
}

/// The path of file `id`, or an error naming the missing file.
fn path_of(files: &SourceFiles, id: SourceFileId) -> Result<&SourcePath, DiagnosticsError> {
    files.get(id).map(|file| &file.path).ok_or_else(|| DiagnosticsError::UnknownFile(id.index()))
}
