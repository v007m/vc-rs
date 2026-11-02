//! `SQLite` result history. Callers only hold a channel sender; a single writer
//! thread owns the connection and writes results in batched transactions.
use std::path::Path;
use std::thread::JoinHandle;

use async_trait::async_trait;
use ports::{Outcome, PortError, PortErrorKind, ResultStore, TestResult};
use rusqlite::Connection;
use tokio::sync::mpsc;

const BATCH: usize = 256;

fn store_error(e: &impl ToString) -> PortError {
    PortError::new(PortErrorKind::Store, e.to_string())
}

/// The cloneable, `Send + Sync` side handed to the engine.
#[derive(Clone)]
pub struct SqliteStore {
    tx: mpsc::Sender<TestResult>,
}

/// Owner of the writer thread; `finish` waits for it to drain.
pub struct StoreWriter {
    thread: JoinHandle<Result<usize, PortError>>,
}

impl SqliteStore {
    /// Open (or create) the database and start the writer thread.
    ///
    /// # Errors
    /// If the database cannot be opened or the schema cannot be created.
    pub fn open(path: &Path, capacity: usize) -> Result<(Self, StoreWriter), PortError> {
        let conn = Connection::open(path).map_err(|e| store_error(&e))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS results(
               test_id INTEGER NOT NULL, name TEXT NOT NULL, outcome TEXT NOT NULL,
               detail TEXT, duration_us INTEGER NOT NULL);",
        )
        .map_err(|e| store_error(&e))?;
        let (tx, rx) = mpsc::channel(capacity.max(1));
        let thread = std::thread::spawn(move || write_loop(conn, rx));
        Ok((Self { tx }, StoreWriter { thread }))
    }
}

impl StoreWriter {
    /// Wait until every sender is dropped and all results are written.
    ///
    /// # Errors
    /// The first write error, or a panic in the writer thread.
    pub fn finish(self) -> Result<usize, PortError> {
        self.thread.join().map_err(|_| PortError::new(PortErrorKind::Store, "writer thread panicked"))?
    }
}

#[async_trait]
impl ResultStore for SqliteStore {
    async fn record(&self, result: TestResult) -> Result<(), PortError> {
        self.tx.send(result).await.map_err(|e| store_error(&e))
    }
}

fn write_loop(mut conn: Connection, mut rx: mpsc::Receiver<TestResult>) -> Result<usize, PortError> {
    let mut written = 0;
    let mut batch = Vec::with_capacity(BATCH);
    while let Some(first) = rx.blocking_recv() {
        batch.push(first);
        while batch.len() < BATCH {
            let Ok(next) = rx.try_recv() else { break };
            batch.push(next);
        }
        written += write_batch(&mut conn, &batch)?;
        batch.clear();
    }
    Ok(written)
}

fn write_batch(conn: &mut Connection, batch: &[TestResult]) -> Result<usize, PortError> {
    let txn = conn.transaction().map_err(|e| store_error(&e))?;
    {
        let mut stmt = txn
            .prepare_cached(
                "INSERT INTO results(test_id, name, outcome, detail, duration_us) VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|e| store_error(&e))?;
        for r in batch {
            let (outcome, detail) = describe(&r.outcome);
            let micros = i64::try_from(r.duration.as_micros()).unwrap_or(i64::MAX);
            stmt.execute((r.test.get(), r.name.as_str(), outcome, detail, micros)).map_err(|e| store_error(&e))?;
        }
    }
    txn.commit().map_err(|e| store_error(&e))?;
    Ok(batch.len())
}

fn describe(outcome: &Outcome) -> (&str, Option<String>) {
    match outcome {
        Outcome::Passed => ("passed", None),
        Outcome::Failed(reasons) => ("failed", Some(reasons.join("; "))),
        Outcome::Errored(message) => ("errored", Some(message.clone())),
    }
}
