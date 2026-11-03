//! The link stage: every file's unit at once. It checks the ID tables and references between
//! files, follows each spot's vars per profile, and finds `test:` cycles; the build stage then
//! turns the units into the program.

mod cycles;
mod flow;
mod ids;
mod layer;
mod project;
mod refs;
mod saves;
mod shared;
mod uses;

use std::collections::HashSet;

use vc_diagnostics::Diagnostics;
use vc_model::{FileKind, SourceFiles, StringTable};

use crate::cache::CacheEntry;
use project::{Project, Source, file_id};

/// Links the units in `entries`, whose spans point into `files`; gives what it found.
///
/// The ID, reference and profile checks always run. The checks that follow vars and reuse run
/// only when no file has an error of its own, since a broken file's items may be missing.
pub fn link(entries: &[CacheEntry], files: &SourceFiles, strings: &StringTable) -> Diagnostics {
    let (sources, unreadable) = sources(entries, files);
    let clean = entries.iter().filter_map(|entry| entry.unit.as_deref()).all(|unit| !unit.diagnostics.has_errors());
    let project = Project::new(sources, strings, unreadable);
    let mut diagnostics = Diagnostics::new();
    ids::check(&project, &mut diagnostics);
    refs::check(&project, &mut diagnostics);
    diagnostics.extend(shared::profiles(&project));
    if clean {
        diagnostics.extend(flow::check(&project));
        cycles::check(&project, &mut diagnostics);
        shared::check(&project, &mut diagnostics);
    }
    diagnostics
}

/// The files with a tree, and the IDs of those the per-file stages could not read cleanly.
fn sources<'a>(entries: &'a [CacheEntry], files: &'a SourceFiles) -> (Vec<Source<'a>>, HashSet<String>) {
    let mut sources = Vec::with_capacity(entries.len());
    let mut unreadable = HashSet::new();
    for entry in entries {
        let (Some(unit), Some(file)) = (entry.unit.as_deref(), files.get(entry.file)) else { continue };
        let Some(kind) = unit.kind else { continue };
        if unit.diagnostics.has_errors() {
            unreadable.insert(if kind == FileKind::Config { String::new() } else { file_id(file.path.as_str(), kind) });
        }
        sources.extend(Source::new(entry.file, file, unit));
    }
    (sources, unreadable)
}
