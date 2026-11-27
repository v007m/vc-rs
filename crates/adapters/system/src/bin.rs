//! Where [`FsSources`](crate::FsSources) sends a file the editor trashes: the system's trash (D683).

use std::fmt::Debug;
use std::path::Path;

/// Takes a file, or a link, out of the project without deleting it.
pub trait Bin: Debug + Send + Sync {
    /// Moves the file or link at `file`, an absolute path, away, and gives where it went as
    /// the system names it, or nothing when the system does not say.
    ///
    /// # Errors
    ///
    /// When it was not moved, in which case it is where it was.
    fn put(&self, file: &Path) -> std::io::Result<String>;
}

/// The system's own trash: the Trash on macOS, through `NSFileManager`, which asks for no
/// permission; the Recycle Bin on Windows; the freedesktop trash elsewhere. Where there is no
/// trash, as on a Linux host with none, the file is refused, never deleted instead.
#[derive(Debug)]
pub struct SystemTrash;

impl Bin for SystemTrash {
    fn put(&self, file: &Path) -> std::io::Result<String> {
        context().delete(file).map_err(|_| std::io::Error::other("the system trash did not take it"))?;
        Ok(String::new())
    }
}

#[cfg(target_os = "macos")]
fn context() -> trash::TrashContext {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    let mut context = trash::TrashContext::default();
    context.set_delete_method(DeleteMethod::NsFileManager);
    context
}

#[cfg(not(target_os = "macos"))]
fn context() -> trash::TrashContext {
    trash::TrashContext::default()
}
