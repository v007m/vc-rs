//! Which profiles a run applies: the ones named (`--profile`, `VIB_PROFILE`), else the ones the
//! config marks `default: true`. The engine and the playground both pick them here.

use vc_model::{ProfileId, Program};

use crate::PlanError;

/// The active profiles: their IDs, in order, and their names for the plan line.
pub type ActiveProfiles = (Vec<ProfileId>, Vec<String>);

/// The profiles `named` names, in order; when it names none, the ones marked `default: true`.
///
/// # Errors
///
/// [`PlanError::UnknownProfile`] for a name the project has no profile for, and
/// [`PlanError::NoProfile`] when the project has profiles, none is a default and none is named.
pub fn active_profiles(program: &Program, named: &[String]) -> Result<ActiveProfiles, PlanError> {
    if named.is_empty() {
        return defaults(program);
    }
    let mut ids = Vec::with_capacity(named.len());
    for name in named {
        let found = program.profiles.keys().copied().find(|id| program.str(id.sym()) == Some(name));
        let Some(id) = found else {
            return Err(PlanError::UnknownProfile { name: name.clone(), known: known(program) });
        };
        ids.push(id);
    }
    Ok((ids, named.to_vec()))
}

/// The profiles marked `default: true`; a project with profiles but no default needs one named.
fn defaults(program: &Program) -> Result<ActiveProfiles, PlanError> {
    let ids: Vec<ProfileId> = program.profiles.values().filter(|p| p.default).map(|p| p.id).collect();
    if ids.is_empty() && !program.profiles.is_empty() {
        return Err(PlanError::NoProfile { known: known(program) });
    }
    let names = ids.iter().map(|id| program.str(id.sym()).unwrap_or_default().to_owned()).collect();
    Ok((ids, names))
}

/// The project's profile names, comma-separated.
fn known(program: &Program) -> String {
    let names: Vec<&str> = program.profiles.keys().filter_map(|id| program.str(id.sym())).collect();
    names.join(", ")
}
