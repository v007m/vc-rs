use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use vc_interfaces::{HistoryError, HistoryWriter, Row, RunEnd, RunId, RunStart};
use vc_model::Keep;

use crate::codec::Location;
use crate::{insert, schema};

/// The longest queued rows wait before they are committed.
const BATCH_WAIT: Duration = Duration::from_millis(200);

/// Queued rows are committed once there are this many.
const BATCH_ROWS: usize = 500;

/// The history writer.
///
/// One thread owns the connection, and every call sends it a request, so there is a single
/// writer without a lock (D170). Dropping every handle commits what is queued and ends the
/// thread.
#[derive(Clone, Debug)]
pub struct SqliteHistory {
    requests: Sender<Request>,
}

#[derive(Debug)]
enum Request {
    Begin(RunStart, Sender<Result<RunId, HistoryError>>),
    Record(RunId, Vec<Row>),
    Finish(RunId, RunEnd, Sender<Result<(), HistoryError>>),
    Prune(Keep, Sender<Result<usize, HistoryError>>),
}

impl SqliteHistory {
    /// Opens the history at `path`, creating and migrating it as needed, and starts its writer.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the file cannot be created, opened or migrated.
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        let connection = schema::open_for_writing(path)?;
        let mut actor =
            Actor { connection, location: Location::of(path), queued: Vec::new(), since: None, failure: None };
        let (requests, inbox) = mpsc::channel();
        std::thread::spawn(move || actor.serve(&inbox));
        Ok(Self { requests })
    }

    fn ask<T>(&self, ask: impl FnOnce(Sender<Result<T, HistoryError>>) -> Request) -> Result<T, HistoryError> {
        let (reply, answer) = mpsc::channel();
        self.requests.send(ask(reply)).map_err(|_| HistoryError::Stopped)?;
        answer.recv().map_err(|_| HistoryError::Stopped)?
    }
}

impl HistoryWriter for SqliteHistory {
    fn begin(&self, start: &RunStart) -> Result<RunId, HistoryError> {
        self.ask(|reply| Request::Begin(start.clone(), reply))
    }

    fn record(&self, run: RunId, rows: Vec<Row>) -> Result<(), HistoryError> {
        self.requests.send(Request::Record(run, rows)).map_err(|_| HistoryError::Stopped)
    }

    fn finish(&self, run: RunId, end: RunEnd) -> Result<(), HistoryError> {
        self.ask(|reply| Request::Finish(run, end, reply))
    }

    fn prune(&self, keep: Keep) -> Result<usize, HistoryError> {
        self.ask(|reply| Request::Prune(keep, reply))
    }
}

/// The writer thread's state.
struct Actor {
    connection: Connection,
    location: Location,
    queued: Vec<(RunId, Vec<Row>)>,
    /// When the oldest queued rows arrived.
    since: Option<Instant>,
    /// The first failed commit since the last finish, reported by the next finish.
    failure: Option<HistoryError>,
}

impl Actor {
    fn serve(&mut self, inbox: &Receiver<Request>) {
        loop {
            let next = self.since.map_or_else(
                || inbox.recv().map_err(|_| RecvTimeoutError::Disconnected),
                |since| inbox.recv_timeout(BATCH_WAIT.saturating_sub(since.elapsed())),
            );
            match next {
                Ok(request) => self.handle(request),
                Err(RecvTimeoutError::Timeout) => self.commit(),
                Err(RecvTimeoutError::Disconnected) => return self.commit(),
            }
        }
    }

    fn handle(&mut self, request: Request) {
        match request {
            Request::Record(run, rows) => self.queue(run, rows),
            Request::Begin(start, reply) => {
                self.commit();
                drop(reply.send(insert::begin(&self.connection, &start).map_err(|error| self.location.error(&error))));
            }
            Request::Finish(run, end, reply) => drop(reply.send(self.finish(run, &end))),
            Request::Prune(keep, reply) => {
                self.commit();
                drop(reply.send(insert::prune(&self.connection, keep).map_err(|error| self.location.error(&error))));
            }
        }
    }

    fn queue(&mut self, run: RunId, rows: Vec<Row>) {
        self.since.get_or_insert_with(Instant::now);
        self.queued.push((run, rows));
        if self.queued.iter().map(|(_, rows)| rows.len()).sum::<usize>() >= BATCH_ROWS {
            self.commit();
        }
    }

    /// Commits everything queued in one transaction; a failure is kept for the next finish.
    fn commit(&mut self) {
        self.since = None;
        if self.queued.is_empty() {
            return;
        }
        let queued = std::mem::take(&mut self.queued);
        let committed = self.connection.transaction().and_then(|transaction| {
            queued.iter().try_for_each(|(run, rows)| insert::rows(&transaction, *run, rows))?;
            transaction.commit()
        });
        if let Err(error) = committed {
            self.failure.get_or_insert_with(|| self.location.error(&error));
        }
    }

    fn finish(&mut self, run: RunId, end: &RunEnd) -> Result<(), HistoryError> {
        self.commit();
        let finished = self.connection.transaction().and_then(|transaction| {
            insert::finish(&transaction, run, end)?;
            transaction.commit()
        });
        let finished = finished.map_err(|error| self.location.error(&error));
        self.failure.take().map_or(finished, Err)
    }
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
