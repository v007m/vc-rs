//! Store adapter: the run history in `.vibranium/history.db` (SQLite, D170).
//!
//! [`SqliteHistory`] is the `HistoryWriter`: one thread owns the write connection and is fed
//! through a channel, committing queued rows in batches (every 200 ms or 500 rows) in WAL mode.
//! [`SqliteHistoryReader`] is the `HistoryReader`: each query opens its own read-only
//! connection, which WAL lets run beside the writer, and a missing file reads as an empty
//! history. The schema is created and upgraded by numbered migrations kept in the file
//! (`schema_migrations`). Bodies and the report snapshot are stored zstd-compressed. Nothing is
//! masked here: rows arrive redacted and cut.

mod codec;
mod insert;
mod reader;
mod schema;
mod writer;

pub use reader::SqliteHistoryReader;
pub use writer::SqliteHistory;
