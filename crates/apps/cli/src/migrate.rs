//! `vc migrate v1 <path>`: converts a Vibranium v1 project into v2 files (D194, D200).

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use clap::Subcommand;
use vc_engine::EngineBuilder;

use crate::Outcome;
use crate::stdout;

/// `vc migrate …`.
#[derive(Subcommand)]
pub enum MigrateCommand {
    /// Converts the Vibranium v1 project at a path into v2 definition files.
    V1 {
        /// The v1 project folder, inside the folder the v2 project is written to (the working
        /// folder, or `--project`), which must be a folder without a project.
        path: PathBuf,
    },
}

/// `vc migrate v1 <path>`: writes the v2 project into the working folder (or `--project`),
/// then lists each file, each `# TODO(vc migrate)` it left and each note. Exit 0 once written;
/// 2 when a project is here already or the folder holds no v1 project, and then nothing is
/// written.
pub fn run(command: &MigrateCommand, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let MigrateCommand::V1 { path } = command;
    let cwd = std::env::current_dir()?;
    let root = project.map_or_else(|| cwd.clone(), |dir| cwd.join(dir));
    let Some(folder) = relative(&root, &cwd.join(path)) else {
        let reason = format!("{} is not inside {}, where the v2 project is written", path.display(), root.display());
        let help = "move the v1 folder into it, or pass `--project <dir>` for a folder that holds it";
        return Ok(Outcome::Helped(reason, Some(help.to_owned())));
    };
    let engine = match EngineBuilder::new(root.clone()).build() {
        Ok(engine) => engine,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    let changes = match engine.migrate_v1(&folder).and_then(|changes| engine.write(&changes).map(|()| changes)) {
        Ok(changes) => changes,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    let mut out = stdout::out();
    writeln!(out, "✓ migrated {} into {}", path.display(), root.display())?;
    for title in &changes.titles {
        writeln!(out, "  {title}")?;
    }
    writeln!(out, "next: vc check")?;
    Ok(Outcome::Done)
}

/// `path` relative to `root` with `/` separators; `None` when it is outside it.
fn relative(root: &Path, path: &Path) -> Option<String> {
    let rest = path.strip_prefix(root).ok()?;
    let mut words = Vec::new();
    for component in rest.components() {
        match component {
            Component::Normal(word) => words.push(word.to_str()?.to_owned()),
            Component::ParentDir => drop(words.pop()?),
            _ => {}
        }
    }
    Some(words.join("/"))
}
