use std::path::Path;
use std::sync::mpsc::{self, Sender};

use rusqlite::{Connection, OptionalExtension, Row, params};
use vc_interfaces::{CacheEntry, CacheKey, CacheListing, CacheStore, CacheStoreError};

use crate::cache_codec::{Columns, decode_cleanups, decode_values, encode_cleanups, encode_values};
use crate::crypt::{CacheSecret, Sealer};
use crate::schema;

/// The schema of `cache.db`, one numbered step per entry; a step never changes once released.
const MIGRATIONS: &[&str] = &[r"
CREATE TABLE entries (
  key BLOB PRIMARY KEY, label TEXT NOT NULL, profiles TEXT NOT NULL,
  created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL,
  saved BLOB NOT NULL, cleanups BLOB NOT NULL
);
CREATE INDEX entries_by_expiry ON entries(expires_at);
"];

/// Every column of an entry, in the order [`Shelf::entry`] reads them.
const COLUMNS: &str = "key, label, profiles, created_at, expires_at, saved, cleanups";

/// A job the store's thread runs against its connection.
type Job = Box<dyn FnOnce(&mut Shelf) + Send>;

/// The cross-run cache in `.vibranium/cache.db` (SQLite). One thread owns the connection and
/// runs the jobs each call sends it, so there is a single writer without a lock. Saved values
/// and cleanups are sealed with ChaCha20-Poly1305 under the cache key, bound to their row and
/// column; a row that no longer opens (another key, a changed byte) reads as a miss and is
/// dropped.
#[derive(Clone, Debug)]
pub struct SqliteCache {
    jobs: Sender<Job>,
}

impl SqliteCache {
    /// Opens the cache at `path` under `secret`, creating and migrating it as needed.
    ///
    /// # Errors
    ///
    /// [`CacheStoreError::Open`] when the file cannot be created, opened or migrated.
    pub fn open(path: &Path, secret: &CacheSecret) -> Result<Self, CacheStoreError> {
        let open_error =
            |error: schema::OpenError| CacheStoreError::Open { location: path.display().to_string(), reason: error.reason() };
        let connection = schema::open_migrated(path, MIGRATIONS).map_err(open_error)?;
        let mut shelf = Shelf { connection, sealer: Sealer::new(secret) };
        let (jobs, inbox) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            for job in inbox {
                job(&mut shelf);
            }
        });
        Ok(Self { jobs })
    }

    fn ask<T: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Shelf) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T, CacheStoreError> {
        let (reply, answer) = mpsc::channel();
        let job: Job = Box::new(move |shelf| drop(reply.send(job(shelf))));
        self.jobs.send(job).map_err(|_| CacheStoreError::Stopped)?;
        let done = answer.recv().map_err(|_| CacheStoreError::Stopped)?;
        done.map_err(|error| CacheStoreError::Failed { reason: error.to_string() })
    }
}

impl CacheStore for SqliteCache {
    fn get(&self, key: &CacheKey, now_ms: u64) -> Result<Option<CacheEntry>, CacheStoreError> {
        let key = *key;
        self.ask(move |shelf| shelf.get(key, now_ms))
    }

    fn put(&self, entry: CacheEntry) -> Result<(), CacheStoreError> {
        self.ask(move |shelf| shelf.put(&entry))
    }

    fn take_expired(&self, now_ms: u64) -> Result<Vec<CacheEntry>, CacheStoreError> {
        self.ask(move |shelf| shelf.take("expires_at <= ?1", now_ms))
    }

    fn list(&self) -> Result<Vec<CacheListing>, CacheStoreError> {
        self.ask(|shelf| shelf.list())
    }

    fn clear(&self, label: Option<&str>) -> Result<Vec<CacheEntry>, CacheStoreError> {
        match label.map(str::to_owned) {
            Some(label) => self.ask(move |shelf| shelf.take("label = ?1", label)),
            None => self.ask(|shelf| shelf.take("1 = ?1", 1_i64)),
        }
    }
}

/// The thread's connection and sealer.
struct Shelf {
    connection: Connection,
    sealer: Sealer,
}

impl Shelf {
    fn get(&self, key: CacheKey, now_ms: u64) -> rusqlite::Result<Option<CacheEntry>> {
        let sql = format!("SELECT {COLUMNS} FROM entries WHERE key = ?1 AND expires_at > ?2");
        let row = self.connection.query_row(&sql, params![key.bytes(), now_ms], |row| entry(&self.sealer, row)).optional()?;
        match row {
            Some(None) => self.forget(key).map(|()| None),
            other => Ok(other.flatten()),
        }
    }

    fn put(&self, entry: &CacheEntry) -> rusqlite::Result<()> {
        let key = entry.key.bytes();
        let saved = self.sealed(key, Columns::Saved, &encode_values(&entry.saved))?;
        let cleanups = self.sealed(key, Columns::Cleanups, &encode_cleanups(&entry.cleanups))?;
        let listing = &entry.listing;
        let sql = format!("INSERT OR REPLACE INTO entries ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)");
        let values = params![key, listing.label, listing.profiles, listing.created_ms, listing.expires_ms, saved, cleanups];
        self.connection.execute(&sql, values).map(drop)
    }

    /// Removes the rows `filter` picks and gives back the ones that still open, oldest first.
    fn take(&mut self, filter: &str, value: impl rusqlite::ToSql) -> rusqlite::Result<Vec<CacheEntry>> {
        let Self { connection, sealer } = self;
        let transaction = connection.transaction()?;
        let sql = format!("SELECT {COLUMNS} FROM entries WHERE {filter} ORDER BY created_at, key");
        let rows = {
            let mut statement = transaction.prepare(&sql)?;
            let read = statement.query_map([&value], |row| entry(sealer, row))?;
            read.collect::<rusqlite::Result<Vec<_>>>()?
        };
        transaction.execute(&format!("DELETE FROM entries WHERE {filter}"), [&value])?;
        transaction.commit()?;
        Ok(rows.into_iter().flatten().collect())
    }

    fn list(&self) -> rusqlite::Result<Vec<CacheListing>> {
        let sql = "SELECT label, profiles, created_at, expires_at FROM entries ORDER BY created_at, key";
        let mut statement = self.connection.prepare(sql)?;
        let listed = statement.query_map([], |row| listing(row, 0))?;
        listed.collect()
    }

    fn forget(&self, key: CacheKey) -> rusqlite::Result<()> {
        self.connection.execute("DELETE FROM entries WHERE key = ?1", [key.bytes()]).map(drop)
    }

    fn sealed(&self, key: &[u8; 32], column: Columns, plain: &[u8]) -> rusqlite::Result<Vec<u8>> {
        let sealed = self.sealer.seal(&column.aad(key), plain);
        sealed.ok_or_else(|| rusqlite::Error::ToSqlConversionFailure("the cache cipher refused a value".into()))
    }
}

/// The entry in `row`; `None` when its values no longer open or decode.
fn entry(sealer: &Sealer, row: &Row<'_>) -> rusqlite::Result<Option<CacheEntry>> {
    let bytes: Vec<u8> = row.get(0)?;
    let Ok(key) = <[u8; 32]>::try_from(bytes.as_slice()) else { return Ok(None) };
    let saved: Vec<u8> = row.get(5)?;
    let cleanups: Vec<u8> = row.get(6)?;
    let saved = sealer.open(&Columns::Saved.aad(&key), &saved).and_then(|plain| decode_values(&plain));
    let cleanups = sealer.open(&Columns::Cleanups.aad(&key), &cleanups).and_then(|plain| decode_cleanups(&plain));
    let listing = listing(row, 1)?;
    Ok(saved.zip(cleanups).map(|(saved, cleanups)| CacheEntry { key: CacheKey::new(key), listing, saved, cleanups }))
}

/// The listing in the four columns of `row` from `first`.
fn listing(row: &Row<'_>, first: usize) -> rusqlite::Result<CacheListing> {
    Ok(CacheListing {
        label: row.get(first)?,
        profiles: row.get(first + 1)?,
        created_ms: row.get(first + 2)?,
        expires_ms: row.get(first + 3)?,
    })
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
