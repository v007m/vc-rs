use std::process::ExitCode;

/// Where to report a bug in Vibranium itself.
const ISSUES: &str = "https://github.com/v007m/vc-rs/issues";

/// How a command ended, when it did not fail inside Vibranium.
pub enum Outcome {
    /// It did what was asked: exit 0.
    Done,
    /// It ran and found errors, which it has printed: exit 2.
    Failed,
    /// It could not start, for the reason given: exit 2.
    Refused(String),
}

/// Prints what the user needs to know and turns the result into the exit code: 0, 2, or 70
/// with a "please report" hint for an error inside Vibranium (D127).
pub fn finish(result: anyhow::Result<Outcome>) -> ExitCode {
    match result {
        Ok(Outcome::Done) => ExitCode::SUCCESS,
        Ok(Outcome::Failed) => ExitCode::from(2),
        Ok(Outcome::Refused(reason)) => {
            eprintln!("error: {reason}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("error: internal error in vc: {error:#}");
            eprintln!("  = help: this is a bug in Vibranium; please report it at {ISSUES}");
            ExitCode::from(70)
        }
    }
}
