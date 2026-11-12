//! The editor's work for the Explore tab (A5): opening a project file, drafting an edit of it
//! through the compile guard, and writing the draft. Each runs on the blocking thread a read
//! is carried out on ([`crate::driver::Driver`]).

use std::time::Duration;

use vc_engine::{Changes, Engine, SaveFile};

use crate::driver::{explore, told};
use crate::screen::Update;

/// The text of the file at `path`, with its digest.
pub fn opened(engine: &Engine, path: String) -> Update {
    match engine.read_file(&path) {
        Ok(file) => Update::Opened { path, text: file.text, digest: file.digest },
        Err(error) => Update::EditFailed(told(&error)),
    }
}

/// The change an edit makes, passed by the compile guard, nothing written; or why it is
/// refused, with the diagnostics it would add.
pub fn drafted_edit(engine: &Engine, request: &SaveFile) -> Update {
    engine.edit_file(request).map_or_else(|error| Update::EditFailed(told(&error)), Update::Edited)
}

/// Writes a drafted edit, then reads the project again, in the files changed in the last
/// `window` when one is given.
pub fn saved(engine: &Engine, changes: &Changes, window: Option<Duration>) -> Vec<Update> {
    match engine.write(changes) {
        Ok(()) => vec![Update::Saved(changes.titles.clone()), explore(engine, window)],
        Err(error) => vec![Update::EditFailed(told(&error))],
    }
}
