//! Finding the project: from `--project <dir>` when given, else from the working folder; and
//! the folder relative paths and selectors start from.

use std::path::{Path, PathBuf};

use vc_engine::{Engine, EngineBuilder};

use crate::Outcome;

/// The project's engine, and where relative paths start: the project root with `--project`,
/// else the working folder.
pub struct Located {
    pub engine: Engine,
    pub base: PathBuf,
}

/// The project at or above `project`, else at or above the working folder.
///
/// # Errors
///
/// [`Outcome::Helped`] when the folder does not exist, [`Outcome::Engine`] when no project
/// holds it; an `Err` when the working folder cannot be read.
pub fn locate(project: Option<&Path>) -> anyhow::Result<Result<Located, Outcome>> {
    let cwd = std::env::current_dir()?;
    let start = match project {
        Some(dir) if !cwd.join(dir).is_dir() => return Ok(Err(missing(dir))),
        Some(dir) => cwd.join(dir),
        None => cwd.clone(),
    };
    let engine = match EngineBuilder::for_project(&start).and_then(EngineBuilder::build) {
        Ok(engine) => engine,
        Err(error) => return Ok(Err(Outcome::Engine(error))),
    };
    let base = if project.is_some() { engine.root().to_path_buf() } else { cwd };
    Ok(Ok(Located { engine, base }))
}

/// The project root at or above `dir`, for commands that need only the folder.
///
/// # Errors
///
/// As [`locate`].
pub fn root(dir: &Path) -> anyhow::Result<Result<PathBuf, Outcome>> {
    Ok(locate(Some(dir))?.map(|located| located.engine.root().to_path_buf()))
}

fn missing(dir: &Path) -> Outcome {
    let help = "pass the folder that holds `vibranium.config.yaml`, or a folder inside it".to_owned();
    Outcome::Helped(format!("--project {}: no such folder", dir.display()), Some(help))
}

impl Located {
    /// Each path relative to the project root with `/` separators, resolved from the base.
    ///
    /// # Errors
    ///
    /// The path that is not a file or folder in the project.
    pub fn project_paths(&self, paths: &[PathBuf]) -> Result<Vec<String>, String> {
        paths.iter().map(|path| self.project_path(path)).collect()
    }

    /// `path` relative to the project root, when it is a file or folder in the project.
    pub fn project_path(&self, path: &Path) -> Result<String, String> {
        self.engine.project_path(&self.base, path).ok_or_else(|| {
            format!("{}: not a file or folder in the project at {}", path.display(), self.engine.root().display())
        })
    }
}
