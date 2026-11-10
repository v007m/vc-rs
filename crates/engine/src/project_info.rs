//! What the web UI shows of the project before anything runs (M9b): its name, its profiles and
//! which a run uses when none is chosen.

use vc_model::Program;

use crate::{Engine, EngineError};

/// The project at a glance.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProjectInfo {
    /// The config's `project`, else the project folder's name.
    pub name: String,
    /// The profiles the config defines, in the order written.
    pub profiles: Vec<String>,
    /// The profiles marked `default: true`, which a run uses when none is chosen.
    pub defaults: Vec<String>,
    /// This engine's version.
    pub vc_version: String,
}

impl Engine {
    /// The project's name and profiles. A project with errors still has a name; its profiles
    /// are those the compile could read.
    ///
    /// # Errors
    ///
    /// As [`Engine::check`] when the project cannot be read.
    pub fn project_info(&self) -> Result<ProjectInfo, EngineError> {
        let compiled = self.compiled()?;
        let folder = self.root().file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let mut info =
            ProjectInfo { name: folder, vc_version: env!("CARGO_PKG_VERSION").to_owned(), ..ProjectInfo::default() };
        if let Some(program) = compiled.program {
            fill(&program, &mut info);
        }
        Ok(info)
    }
}

/// `info`'s name, profiles and defaults from `program`.
fn fill(program: &Program, info: &mut ProjectInfo) {
    if let Some(name) = &program.config.project {
        info.name.clone_from(name);
    }
    for profile in program.profiles.values() {
        let name = program.str(profile.id.sym()).unwrap_or_default().to_owned();
        if profile.default {
            info.defaults.push(name.clone());
        }
        info.profiles.push(name);
    }
}
