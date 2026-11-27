//! The report folder a client of `vc serve` names for a run: kept inside the project (D671).

use std::path::{Component, Path, PathBuf};

use vc_adapter_reporters::resolves_inside;
use vc_diagnostics::Code;

use crate::{Engine, EngineError};

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
        let out = Path::new(out);
        let plain = out.components().all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
        if plain && resolves_inside(self.root(), &self.root().join(out)) {
            return Ok(out.to_path_buf());
        }
        Err(EngineError::Reporter {
            reason: format!("{}: `out` is not a folder inside the project", Code::V5132),
            help: Some("name a folder relative to the project root, with no `..` and no link out of it".to_owned()),
        })
    }
}

#[cfg(test)]
#[path = "client_out_tests.rs"]
mod tests;
