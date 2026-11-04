//! The link stage: every file's unit at once. It checks the ID tables and references between
//! files, follows each spot's vars per profile, and finds `test:` cycles; the build stage then
//! turns the units into the program, and the lint stage checks the lifecycle rules over both.

mod assets;
mod build;
mod columns;
mod cycles;
mod fakes;
mod flow;
mod ids;
mod layer;
mod project;
mod refs;
mod saves;
mod shared;
mod uses;

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use vc_diagnostics::Diagnostics;
use vc_model::{Date, FileKind, Program, SourceFiles, SourcePath, StringTable};

pub use build::{flag, method};
pub use project::{Project, Source, TEST_BLOCKS, actions, list, text};
pub use uses::piece_refs;

use crate::cache::CacheEntry;
use crate::lint::{LintCx, LintRegistry};

/// What linking found, and the program when nothing is an error.
pub struct Linked {
    /// The link stage's diagnostics.
    pub diagnostics: Diagnostics,
    /// The program, built only when no file and no link check has an error.
    pub program: Option<Arc<Program>>,
}

/// What linking is told besides the units: the names they share, and what the files cannot say.
#[derive(Clone, Copy)]
pub struct World<'w> {
    /// The names the units use.
    pub strings: &'w StringTable,
    /// Today's date in UTC, for the lints that compare dates.
    pub today: Option<Date>,
    /// The project's other files, for `$file` paths; `None` leaves them unchecked.
    pub assets: Option<&'w BTreeSet<SourcePath>>,
}

/// Links the units in `entries`, whose spans point into `files`.
///
/// The ID, reference, `$file` (when `world` lists the assets), example column and profile
/// checks always run. The checks that follow vars and reuse run,
/// and the program is built, only when no file has an error of its own, since a broken file's
/// items may be missing. The lints run over a built program; an error among them drops it.
pub fn link(entries: &[CacheEntry], files: &SourceFiles, world: &World<'_>) -> Linked {
    let (sources, unreadable) = sources(entries, files);
    let clean = entries.iter().filter_map(|entry| entry.unit.as_deref()).all(|unit| !unit.diagnostics.has_errors());
    let project = Project::new(sources, world.strings, unreadable);
    let mut diagnostics = Diagnostics::new();
    ids::check(&project, &mut diagnostics);
    refs::check(&project, &mut diagnostics);
    diagnostics.extend(world.assets.map(|assets| assets::check(&project, files, assets)).unwrap_or_default());
    diagnostics.extend(columns::check(&project));
    diagnostics.extend(fakes::check(&project));
    diagnostics.extend(shared::profiles(&project));
    if !clean {
        return Linked { diagnostics, program: None };
    }
    diagnostics.extend(flow::check(&project));
    cycles::check(&project, &mut diagnostics);
    shared::check(&project, &mut diagnostics);
    let program = if diagnostics.has_errors() { None } else { build::program(&project, files.clone()) };
    if let Some(program) = &program {
        diagnostics.extend(LintRegistry::builtin().run(&LintCx { project: &project, program, today: world.today }));
    }
    let program = program.filter(|_| !diagnostics.has_errors());
    Linked { diagnostics, program }
}

/// The files with a tree, and the IDs of those the per-file stages could not read cleanly.
fn sources<'a>(entries: &'a [CacheEntry], files: &'a SourceFiles) -> (Vec<Source<'a>>, HashSet<String>) {
    let mut sources = Vec::with_capacity(entries.len());
    let mut unreadable = HashSet::new();
    for entry in entries {
        let (Some(unit), Some(file)) = (entry.unit.as_deref(), files.get(entry.file)) else { continue };
        let Some(kind) = unit.kind else { continue };
        if unit.diagnostics.has_errors() {
            unreadable.insert(if kind == FileKind::Config {
                String::new()
            } else {
                project::file_id(file.path.as_str(), kind)
            });
        }
        sources.extend(Source::new(entry.file, file, unit));
    }
    (sources, unreadable)
}
