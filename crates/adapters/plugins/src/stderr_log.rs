//! A plugin's stderr kept in a log in its data folder, so it never reaches a terminal the
//! host's UI is drawing on.

use std::path::PathBuf;

use tokio::io::{AsyncRead, AsyncReadExt};

/// The log's name in the plugin's data folder.
pub const STDERR_LOG: &str = "stderr.log";

/// The previous process's log.
const OLD_LOG: &str = "stderr.log.1";

/// The most of one process's stderr its log keeps; the rest is read and dropped.
pub const LOG_LIMIT: u64 = 1024 * 1024;

/// Copies `stderr` into [`STDERR_LOG`] in `data`, the log before it kept as `stderr.log.1`, up
/// to [`LOG_LIMIT`]. Past the limit, or when the log cannot be written, the rest is read and
/// dropped, so the plugin never blocks on a full pipe.
pub async fn keep(mut stderr: impl AsyncRead + Unpin, data: PathBuf) {
    let log = data.join(STDERR_LOG);
    tokio::fs::rename(&log, data.join(OLD_LOG)).await.ok();
    if let Ok(mut file) = tokio::fs::File::create(&log).await {
        tokio::io::copy(&mut (&mut stderr).take(LOG_LIMIT), &mut file).await.ok();
    }
    tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await.ok();
}
