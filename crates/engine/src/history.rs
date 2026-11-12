//! The run history (`.vibranium/history.db`, D170): recording a run as it goes, and reading
//! the stored runs back for `vc history`, `vc report` and `vc run --failed`. Pruning a run
//! drops its report folder too (`.vibranium/reports/<run>/`), when `vc` marked it as a run's.
//!
//! Everything recorded comes from the event stream after the redactor, so nothing raw reaches
//! the store. A history that cannot be opened or written never changes a run's result: the run
//! goes on and says so in a notice.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use vc_adapter_reporters::prune_run_folders;
use vc_adapter_store::{SqliteHistory, SqliteHistoryReader};
use vc_interfaces::{
    HistoryError, HistoryReader, HistoryWriter, Row, RunEnd, RunFilter, RunId, RunRef, RunRow, RunStart,
};
use vc_model::Keep;

use crate::reporting::REPORTS_DIR;
use crate::{Engine, EngineError, SelectRequest};

/// Where the history lives, under the project root.
const HISTORY: &str = ".vibranium/history.db";

/// A run being written to the history.
pub struct Recording {
    writer: Arc<dyn HistoryWriter>,
    run: RunId,
    reader: Arc<dyn HistoryReader>,
    /// The folder of the runs' report folders, whose pruned runs' folders go too.
    folders: Option<PathBuf>,
}

impl Recording {
    /// The run's number.
    pub const fn id(&self) -> RunId {
        self.run
    }

    /// The same recording, dropping the report folders under `folders` of the runs it prunes.
    #[must_use]
    pub fn pruning(self, folders: Option<PathBuf>) -> Self {
        Self { folders, ..self }
    }

    /// Queues `rows` for the writer.
    ///
    /// # Errors
    ///
    /// [`HistoryError::Stopped`] when the writer is no longer running.
    pub fn record(&self, rows: Vec<Row>) -> Result<(), HistoryError> {
        if rows.is_empty() {
            return Ok(());
        }
        self.writer.record(self.run, rows)
    }

    /// Closes the run with `end`, then drops the oldest runs beyond `keep`, and their report
    /// folders, on a blocking thread.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the store cannot be written or a report folder removed.
    pub async fn close(self, end: RunEnd, keep: Keep) -> Result<(), HistoryError> {
        let Self { writer, run, reader, folders } = self;
        let closed = tokio::task::spawn_blocking(move || {
            writer.finish(run, end)?;
            writer.prune(keep)?;
            prune_folders(reader.as_ref(), keep, (folders.as_deref(), Some(run)))
        });
        closed.await.map_err(|error| HistoryError::Failed { reason: error.to_string() })?
    }
}

impl Engine {
    /// Opens the history and starts recording a run that starts as `start` says.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the history cannot be opened or written.
    pub(crate) fn recording(&self, start: &RunStart) -> Result<Recording, HistoryError> {
        let writer = self.history_writer()?;
        let run = writer.begin(start)?;
        Ok(Recording { writer, run, reader: self.history_reader(), folders: None })
    }

    /// Drops the oldest runs beyond the newest `keep`, else the config's `history.keep`, and
    /// their report folders under `reports.dir`, and says how many runs went.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when the history cannot be opened or written, and the errors of
    /// compiling the project when `keep` is `None`.
    pub fn prune_history(&self, keep: Option<u32>) -> Result<usize, EngineError> {
        let keep = match keep {
            Some(runs) => Keep::Runs(runs),
            None => self.program()?.0.config.history.keep,
        };
        let writer = self.history_writer().map_err(|error| history_error(&error))?;
        let pruned = writer.prune(keep).map_err(|error| history_error(&error))?;
        let folders = self.report_folders();
        let reader = self.history_reader();
        prune_folders(reader.as_ref(), keep, (Some(&folders), None)).map_err(|error| history_error(&error))?;
        Ok(pruned)
    }

    /// Where each run's report folder goes: the layers' `reports.dir`, else the config's, else
    /// `.vibranium/reports`, under the root; the default when the project or a layer has errors.
    fn report_folders(&self) -> PathBuf {
        let layered = self.layered(&[], &[]).ok();
        let set = layered.as_ref().and_then(|layered| layered.settings.reports_dir.clone());
        let configured = || layered.as_ref().and_then(|layered| layered.program.config.reports_dir.clone());
        self.root().join(set.or_else(configured).unwrap_or_else(|| REPORTS_DIR.to_owned()))
    }

    /// The stored run `which` names: `last` or a run number.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when `which` is neither, no such run is stored, or the history
    /// cannot be read.
    pub(crate) fn stored_run(&self, which: &str) -> Result<RunRow, EngineError> {
        let wanted = RunRef::parse(which).ok_or_else(|| EngineError::History {
            reason: format!("`{which}` is not a run"),
            help: Some("pass `last` or a run number from `vc history`".to_owned()),
        })?;
        let found = self.history_reader().run(wanted).map_err(|error| history_error(&error))?;
        found.ok_or_else(|| EngineError::History {
            reason: if which == "last" {
                "no run is stored yet".to_owned()
            } else {
                format!("no run {which} is stored")
            },
            help: Some("`vc history` lists the stored runs".to_owned()),
        })
    }

    /// The writer the builder was given, else the SQLite history under the root.
    fn history_writer(&self) -> Result<Arc<dyn HistoryWriter>, HistoryError> {
        match &self.history {
            Some(writer) => Ok(Arc::clone(writer)),
            None => Ok(Arc::new(SqliteHistory::open(&self.history_path())?)),
        }
    }

    /// The reader the builder was given, else the SQLite history under the root.
    pub(crate) fn history_reader(&self) -> Arc<dyn HistoryReader> {
        self.history_reader.clone().unwrap_or_else(|| Arc::new(SqliteHistoryReader::new(&self.history_path())))
    }

    pub(crate) fn history_path(&self) -> PathBuf {
        self.root().join(HISTORY)
    }
}

/// Removes the marked report folders under `folders` of the runs older than the oldest that
/// `keep` leaves stored, never the folder of `current`, the run that just wrote it: with
/// `keep: 0` every one older than `current`. With `keep: all`, or no run stored and none
/// current, nothing is pruned.
fn prune_folders(
    reader: &dyn HistoryReader,
    keep: Keep,
    (folders, current): (Option<&Path>, Option<RunId>),
) -> Result<(), HistoryError> {
    let (Some(folders), Keep::Runs(runs)) = (folders, keep) else {
        return Ok(());
    };
    let stored = reader.runs(usize::try_from(runs).unwrap_or(usize::MAX), &RunFilter::default())?;
    let oldest = stored.last().map(|row| row.id).or_else(|| current.filter(|_| runs == 0));
    let Some(oldest) = oldest else { return Ok(()) };
    let below = current.map_or(oldest, |current| oldest.min(current));
    prune_run_folders(folders, below).map(drop).map_err(|error| HistoryError::Failed { reason: error.to_string() })
}

/// The selectors of `select` as one line, for the history: IDs and paths, then `-t` and `--grep`.
pub fn selector(select: &SelectRequest) -> String {
    let mut parts: Vec<String> = select.ids.iter().chain(&select.paths).cloned().collect();
    parts.extend(select.tags.iter().map(|tag| format!("-t {tag}")));
    parts.extend(select.grep.iter().map(|grep| format!("--grep {grep}")));
    parts.extend(select.changed.iter().map(|base| format!("--changed {base}").trim_end().to_owned()));
    parts.join(" ")
}

/// A history failure as the engine's error.
pub fn history_error(error: &HistoryError) -> EngineError {
    EngineError::History { reason: error.to_string(), help: None }
}

#[cfg(test)]
mod tests {
    use super::selector;
    use crate::SelectRequest;

    #[test]
    fn the_selector_reads_as_typed() {
        let select = SelectRequest {
            ids: vec!["users.crud".into()],
            paths: vec!["services/orders".into()],
            tags: vec!["smoke".into()],
            grep: Some("email".into()),
            changed: Some(String::new()),
            saved: None,
            include_skipped: false,
        };
        assert_eq!(selector(&select), "users.crud services/orders -t smoke --grep email --changed");
        assert_eq!(selector(&SelectRequest::default()), "");
    }
}
