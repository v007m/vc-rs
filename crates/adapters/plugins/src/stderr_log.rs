//! A plugin's stderr kept in a log in a folder the host owns, so it never reaches a terminal the
//! host's UI is drawing on.
//!
//! The folder is never the plugin's data folder, which the plugin may write to: a link planted
//! there would make the host write over the file it points at.

use std::fs::{File, OpenOptions};
use std::path::Path;

use tokio::io::{AsyncRead, AsyncReadExt};

/// The log's name in its folder.
pub const STDERR_LOG: &str = "stderr.log";

/// The previous process's log.
const OLD_LOG: &str = "stderr.log.1";

/// The most of one process's stderr its log keeps; the rest is read and dropped.
pub const LOG_LIMIT: u64 = 1024 * 1024;

/// A new, empty [`STDERR_LOG`] in `folder` for a process about to start, the log before it kept
/// as `stderr.log.1`; `None` when it cannot be made. The log is created only where nothing is,
/// so a link at its path is never followed (`O_CREAT | O_EXCL`), and on Windows a reparse point
/// is refused.
pub fn open(folder: &Path) -> Option<File> {
    std::fs::create_dir_all(folder).ok()?;
    let log = folder.join(STDERR_LOG);
    std::fs::rename(&log, folder.join(OLD_LOG)).ok();
    let file = OpenOptions::new().write(true).create_new(true).open(&log).ok()?;
    plain(&file).then_some(file)
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
