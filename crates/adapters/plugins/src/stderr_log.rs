//! A plugin's stderr kept in a log in a folder the host owns, so it never reaches a terminal the
//! host's UI is drawing on.
//!
//! The folder is never the plugin's data folder, which the plugin may write to: a link planted
//! there would make the host write over the file it points at. Nor is it ever reached through a
//! link: each folder on the way to it is checked, and a link refused.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use tokio::io::{AsyncRead, AsyncReadExt};

/// The log's name in its folder.
pub const STDERR_LOG: &str = "stderr.log";

/// The previous process's log.
const OLD_LOG: &str = "stderr.log.1";

/// The most of one process's stderr its log keeps; the rest is read and dropped.
pub const LOG_LIMIT: u64 = 1024 * 1024;

/// A new, empty [`STDERR_LOG`] in the folder `under` in `home` ([`log_folder`]) for a process
/// about to start, the log before it kept as `stderr.log.1`.
///
/// The log is created only where nothing is, so a link at its path is never followed
/// (`O_CREAT | O_EXCL`), and on Windows a reparse point is refused. When the log before cannot
/// be kept it is removed, and the new log's first line says so.
///
/// # Errors
///
/// Why the log cannot be made: its folder cannot be used, or the file cannot be created there.
pub fn open(home: &Path, under: &Path) -> io::Result<File> {
    let folder = log_folder(home, under)?;
    let log = folder.join(STDERR_LOG);
    let rotated = rotate(&log, &folder.join(OLD_LOG));
    let mut file = create(&log)?;
    if let Err(error) = rotated {
        writeln!(file, "vibranium: the log before could not be kept as {OLD_LOG} ({error}), so it was removed").ok();
    }
    Ok(file)
}

/// The folder `under` in `home`, each folder on the way made when missing.
///
/// It is refused, with why, when one of them is a link or not a folder, or `under` would leave
/// `home`, so a log is never rotated, removed or made through a link. `home` itself is taken as
/// it is.
///
/// # Errors
///
/// Why the folder cannot be used.
pub fn log_folder(home: &Path, under: &Path) -> io::Result<PathBuf> {
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

/// Makes the folder `path` when nothing is there; a folder already there is used, anything else
/// (a link, a file) is refused.
fn made(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(found) if found.is_dir() && !found.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(io::Error::other(format!("`{}` is a link or not a folder", path.display()))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::create_dir(path),
        Err(error) => Err(error),
    }
}

/// Renames `log` to `old`, over it; when that fails, removes `log` (a link itself, never what
/// it points at) so a new log can be made, and is why it failed. No log yet is no failure.
fn rotate(log: &Path, old: &Path) -> io::Result<()> {
    match std::fs::rename(log, old) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            std::fs::remove_file(log).ok();
            Err(error)
        }
        _ => Ok(()),
    }
}

/// A new file at `log`, never one already there; on Windows opened as the reparse point
/// itself, were one there, so [`plain`] sees the link and not its target. Refused, with why,
/// when something is in the way or it is a link.
fn create(log: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        /// `FILE_FLAG_OPEN_REPARSE_POINT`.
        const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(OPEN_REPARSE_POINT);
    }
    let cannot = |why: String| io::Error::other(format!("`{}` cannot be made: {why}", log.display()));
    let file = options.open(log).map_err(|error| cannot(error.to_string()))?;
    if plain(&file) { Ok(file) } else { Err(cannot("it is a link".to_owned())) }
}

/// Whether `file` is a file of its own, not a reparse point standing for another.
#[cfg(windows)]
fn plain(file: &File) -> bool {
    use std::os::windows::fs::MetadataExt;
    const REPARSE_POINT: u32 = 0x400;
    file.metadata().is_ok_and(|metadata| metadata.file_attributes() & REPARSE_POINT == 0)
}

/// Whether `file` is a file of its own: `create_new` never opens through a link here.
#[cfg(not(windows))]
const fn plain(_: &File) -> bool {
    true
}

/// Copies `stderr` into `log`, up to [`LOG_LIMIT`]. Past the limit, or when the log cannot be
/// written, the rest is read and dropped, so the plugin never blocks on a full pipe.
pub async fn keep(mut stderr: impl AsyncRead + Unpin, log: File) {
    let mut log = tokio::fs::File::from_std(log);
    tokio::io::copy(&mut (&mut stderr).take(LOG_LIMIT), &mut log).await.ok();
    tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await.ok();
}

#[cfg(test)]
#[path = "stderr_log_tests.rs"]
mod tests;
