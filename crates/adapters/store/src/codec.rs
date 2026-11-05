use std::fmt::Display;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::ErrorCode;
use rusqlite::types::{FromSqlError, Type};
use vc_interfaces::HistoryError;
use vc_model::{RunStatus, TestStatus};

/// The zstd level bodies and snapshots are compressed at: fast, and most of the gain.
const ZSTD_LEVEL: i32 = 3;

/// How each test status is spelled in the file.
const TEST_STATUSES: [(TestStatus, &str); 5] = [
    (TestStatus::Passed, "passed"),
    (TestStatus::Failed, "failed"),
    (TestStatus::Blocked, "blocked"),
    (TestStatus::Flaky, "flaky"),
    (TestStatus::Skipped, "skipped"),
];

/// How each run status is spelled in the file.
const RUN_STATUSES: [(RunStatus, &str); 3] =
    [(RunStatus::Passed, "passed"), (RunStatus::Failed, "failed"), (RunStatus::Cancelled, "cancelled")];

/// Where the history is, for errors.
#[derive(Clone, Debug)]
pub struct Location(String);

impl Location {
    pub fn of(path: &Path) -> Self {
        Self(path.display().to_string())
    }

    /// The file could not be opened.
    pub fn open(&self, reason: &dyn Display) -> HistoryError {
        HistoryError::Open { location: self.0.clone(), reason: reason.to_string() }
    }

    /// A statement failed: another process holding the lock past the timeout, or anything else.
    pub fn error(&self, error: rusqlite::Error) -> HistoryError {
        match error.sqlite_error_code() {
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => HistoryError::Busy { location: self.0.clone() },
            _ => HistoryError::Failed { reason: error.to_string() },
        }
    }
}

/// Now, in Unix milliseconds.
pub fn now_ms() -> u64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
}

pub fn test_status_text(status: TestStatus) -> &'static str {
    TEST_STATUSES.iter().find(|(known, _)| *known == status).map_or("failed", |(_, text)| text)
}

pub fn run_status_text(status: RunStatus) -> &'static str {
    RUN_STATUSES.iter().find(|(known, _)| *known == status).map_or("failed", |(_, text)| text)
}

pub fn test_status(text: &str) -> Option<TestStatus> {
    TEST_STATUSES.iter().find(|(_, known)| *known == text).map(|(status, _)| *status)
}

pub fn run_status(text: &str) -> Option<RunStatus> {
    RUN_STATUSES.iter().find(|(_, known)| *known == text).map(|(status, _)| *status)
}

/// `value` as JSON text: profiles and headers.
pub fn json<T: serde::Serialize + ?Sized>(value: &T) -> rusqlite::Result<String> {
    serde_json::to_string(value).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

/// JSON text read back from column `column`.
pub fn from_json<T: serde::de::DeserializeOwned>(column: usize, text: &str) -> rusqlite::Result<T> {
    serde_json::from_str(text).map_err(|error| conversion(column, Box::new(error)))
}

pub fn compress(bytes: &[u8]) -> rusqlite::Result<Vec<u8>> {
    zstd::bulk::compress(bytes, ZSTD_LEVEL).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

/// Decompressed bytes of column `column`; zstd frames record their own size.
pub fn decompress(column: usize, bytes: &[u8]) -> rusqlite::Result<Vec<u8>> {
    zstd::stream::decode_all(bytes).map_err(|error| conversion(column, Box::new(error)))
}

/// A column that holds something this version cannot read.
pub fn unreadable(column: usize, text: &str) -> rusqlite::Error {
    conversion(column, Box::new(FromSqlError::Other(format!("unknown value `{text}`").into())))
}

fn conversion(column: usize, error: Box<dyn std::error::Error + Send + Sync>) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, Type::Text, error)
}

#[cfg(test)]
mod tests {
    use rusqlite::ffi;
    use vc_interfaces::HistoryError;
    use vc_model::{RunStatus, TestStatus};

    use super::{
        Location, compress, decompress, run_status, run_status_text, test_status, test_status_text,
    };

    #[test]
    fn statuses_read_back_as_written() {
        for status in [TestStatus::Passed, TestStatus::Failed, TestStatus::Blocked, TestStatus::Flaky, TestStatus::Skipped] {
            assert_eq!(test_status(test_status_text(status)), Some(status));
        }
        for status in [RunStatus::Passed, RunStatus::Failed, RunStatus::Cancelled] {
            assert_eq!(run_status(run_status_text(status)), Some(status));
        }
        assert_eq!((test_status("weird"), run_status("weird")), (None, None));
    }

    #[test]
    fn bodies_survive_compression() {
        let body = b"{\"id\":7,\"name\":\"Ada\"}".repeat(50);
        assert_eq!(decompress(0, &compress(&body).unwrap()).unwrap(), body);
        assert!(decompress(0, b"not zstd").is_err());
    }

    #[test]
    fn a_lock_held_too_long_says_so() {
        let location = Location::of(std::path::Path::new(".vibranium/history.db"));
        let busy = rusqlite::Error::SqliteFailure(ffi::Error::new(ffi::SQLITE_BUSY), None);
        assert_eq!(location.error(busy), HistoryError::Busy { location: ".vibranium/history.db".into() });
        let other = location.error(rusqlite::Error::InvalidQuery);
        assert!(matches!(other, HistoryError::Failed { .. }));
    }
}
