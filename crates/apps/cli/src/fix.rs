//! `vc check --fix`: shows the certain fixes as a diff, applies them when confirmed, and checks
//! again (D88).

use vc_engine::{CheckRequest, Engine};

use crate::{Outcome, changes, check};

/// Exit 0 when the project has no errors afterwards, 2 when it still has some. With `yes` it
/// applies the fixes without asking.
pub fn run(engine: &Engine, request: &CheckRequest, yes: bool) -> anyhow::Result<Outcome> {
    let fixed = match engine.fix(request) {
        Ok(fixed) => fixed,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    if fixed.changes.is_empty() {
        check::print(&fixed.report, false)?;
        eprintln!("no certain fixes to apply");
        return Ok(check::outcome(&fixed.report));
    }
    changes::preview(&fixed.changes)?;
    let count = fixed.changes.titles.len();
    let wording = if count == 1 { "1 fix".to_owned() } else { format!("{count} fixes") };
    if !yes && !changes::confirm(&format!("Apply {wording}?"))? {
        eprintln!("nothing changed");
        return Ok(check::outcome(&fixed.report));
    }
    if let Err(error) = engine.write(&fixed.changes) {
        return Ok(Outcome::Engine(error));
    }
    eprintln!("✓ applied {wording}");
    match engine.check(request) {
        Ok(report) => {
            check::print(&report, false)?;
            Ok(check::outcome(&report))
        }
        Err(error) => Ok(Outcome::Engine(error)),
    }
}
