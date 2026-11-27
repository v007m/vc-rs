//! The report folder a client of `vc ui`'s server names for a run, as `out` or as a `reports.dir`
//! among its `sets`: kept inside the project (D671, D685).

use std::path::{Component, Path, PathBuf};

use vc_adapter_reporters::resolves_inside;
use vc_adapter_system::SetFlags;
use vc_diagnostics::Code;
use vc_interfaces::ConfigSource;

use crate::{Engine, EngineError};

/// The setting that names the report folder.
const REPORTS_DIR: &str = "reports.dir";

impl Engine {
    /// `out`, a report folder a client names, when it stays inside the project: relative, with
    /// no `..`, and inside the root on disk, links followed. The path is not echoed back, as it
    /// came from the client.
    ///
    /// # Errors
    ///
    /// [`EngineError::Reporter`] with V5132 when it is absolute, has a `..` part, or leads
    /// outside the project on disk.
    pub fn client_report_folder(&self, out: &str) -> Result<PathBuf, EngineError> {
        self.folder_inside(out, "`out`")
    }

    /// Refuses `sets`, the `--set` values a client sends, when one sets `reports.dir` to a folder
    /// [`Engine::client_report_folder`] would refuse (D685). Only such a set reads the disk; a set
    /// that does not parse is left to the layering, which refuses it.
    ///
    /// # Errors
    ///
    /// [`EngineError::Reporter`] with V5132, as [`Engine::client_report_folder`].
    pub fn client_sets(&self, sets: &[String]) -> Result<(), EngineError> {
        for set in sets {
            let entries = SetFlags::new(vec![set.clone()]).entries().unwrap_or_default();
            for entry in entries.iter().filter(|entry| entry.key == REPORTS_DIR) {
                self.folder_inside(&entry.value, "`reports.dir`")?;
            }
        }
        Ok(())
    }

    /// `folder`, which the client names as `name`, when it stays inside the project.
    pub(crate) fn folder_inside(&self, folder: &str, name: &str) -> Result<PathBuf, EngineError> {
        let folder = Path::new(folder);
        let plain = folder.components().all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
        if plain && resolves_inside(self.root(), &self.root().join(folder)) {
            return Ok(folder.to_path_buf());
        }
        Err(EngineError::Reporter {
            reason: format!("{}: {name} is not a folder inside the project", Code::V5132),
            help: Some("name a folder relative to the project root, with no `..` and no link out of it".to_owned()),
        })
    }
}

#[cfg(test)]
#[path = "client_out_tests.rs"]
mod tests;
