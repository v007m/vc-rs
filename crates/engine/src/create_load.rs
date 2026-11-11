//! The body of a new load file from `vc new file load`: a `mix` of one of the project's tests
//! and one short stage, so the file passes `vc check` and `vc load` runs it.

use vc_model::FileKind;

use crate::EngineError;
use crate::authoring::{Authoring, refused};

/// The `mix` and `stages` of a new load file, after its header: the project's first test by ID,
/// run by one user for 30 seconds. Refused when the project has no test, since a mix needs one.
pub fn load_body(project: &Authoring) -> Result<String, EngineError> {
    let mut tests: Vec<String> =
        project.known(FileKind::Test, "tests").into_iter().filter(|id| id.contains('#')).collect();
    tests.sort();
    let Some(test) = tests.first() else {
        return Err(refused(
            "a load file runs tests, and this project has none yet",
            "add one first with `vc new test <fileId>#<testId> --from <api>`",
        ));
    };
    Ok(format!(
        "mix:\n  # replace with the tests to run, each with its share of the traffic\n  - test: {test}\n    \
         weight: 1\nstages:\n  - duration: 30s\n    users: 1\n"
    ))
}
