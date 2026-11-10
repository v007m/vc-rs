//! The folders the host keeps for a plugin under the project's `.vibranium`, its log and data
//! folders, reached only through folders of their own: a link on the way is refused, so nothing
//! the host makes, moves or removes there ever lands where a link points.

use std::io;
use std::path::{Component, Path, PathBuf};

/// The folder `under` in `home`, each folder on the way made when missing.
///
/// It is refused, with why, when one of them is a link or not a folder, or `under` would leave
/// `home`. `home` itself is taken as it is. Two hosts walking the same way at once both get it.
///
/// # Errors
///
/// Why the folder cannot be used.
pub fn folder(home: &Path, under: &Path) -> io::Result<PathBuf> {
    let mut folder = home.to_path_buf();
    for part in under.components() {
        let Component::Normal(name) = part else {
            return Err(io::Error::other(format!("`{}` leaves `{}`", under.display(), home.display())));
        };
        folder.push(name);
        made(&folder)?;
    }
    Ok(folder)
}

/// Makes the folder `path`; when something is already there (made just now by another host, or
/// left before), a folder is used and anything else (a link, a file) is refused.
fn made(path: &Path) -> io::Result<()> {
    match std::fs::create_dir(path) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => own(path),
        made => made,
    }
}

/// Whether `path` is a folder itself, not a link to one.
fn own(path: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(path)?.is_dir() {
        Ok(())
    } else {
        Err(io::Error::other(format!("`{}` is a link or not a folder", path.display())))
    }
}

#[cfg(test)]
#[path = "owned_tests.rs"]
mod tests;
