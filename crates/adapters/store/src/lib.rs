//! Store adapter: the run history in `.vibranium/history.db` (SQLite, D170) and the cross-run
//! cache in `.vibranium/cache.db`.
//!
//! [`SqliteHistory`] is the `HistoryWriter`: one thread owns the write connection and is fed
//! through a channel, committing queued rows in batches (every 200 ms or 500 rows) in WAL mode.
//! [`SqliteHistoryReader`] is the `HistoryReader`: each query opens its own read-only
//! connection, which WAL lets run beside the writer, and a missing file reads as an empty
//! history. The schema is created and upgraded by numbered migrations kept in the file
//! (`schema_migrations`). Bodies and the report snapshot are stored zstd-compressed. Nothing is
//! masked here: rows arrive redacted and cut.
//!
//! [`SqliteApprovals`] is the `Approvals` queue and the `AgentActionLog` in the same
//! file (D195). Each call opens its own connection, so every process on the project shares one
//! queue, and a wait wakes on the folder's file events.
//!
//! [`SqliteCache`] is the `CacheStore`, built the same way: one thread owns its connection.
//! Every saved value and cleanup is sealed with ChaCha20-Poly1305 under a [`CacheSecret`] (from
//! the OS keychain or `VIB_CACHE_KEY`), bound to its row and column, so the file holds no value
//! in the clear and a row that no longer opens reads as a miss.

mod approvals;
mod cache;
mod cache_codec;
mod codec;
mod crypt;
mod insert;
mod reader;
mod schema;
mod writer;

pub use approvals::SqliteApprovals;
pub use cache::SqliteCache;
pub use crypt::CacheSecret;
pub use reader::SqliteHistoryReader;
pub use writer::SqliteHistory;
