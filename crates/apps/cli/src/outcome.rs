use std::process::ExitCode;

use vc_engine::EngineError;

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
    /// It could not start, for the reason given, with what to do instead: exit 2.
    Helped(String, Option<String>),
    /// The engine refused: exit 2, or 70 for an error inside the engine.
    Engine(EngineError),
    /// It ran, and its result is this exit code (D127).
    Exit(u8),
}

/// Prints what the user needs to know and turns the result into the exit code: 0, 1, 2, 130,
/// or 70 with a "please report" hint for an error inside Vibranium (D127).
pub fn finish(result: anyhow::Result<Outcome>) -> ExitCode {
    match result {
        Ok(Outcome::Done) => ExitCode::SUCCESS,
        Ok(Outcome::Failed) => ExitCode::from(2),
        Ok(Outcome::Exit(code)) => ExitCode::from(code),
        Ok(Outcome::Refused(reason)) => refuse(&reason, None),
        Ok(Outcome::Helped(reason, help)) => refuse(&reason, help.as_deref()),
        Ok(Outcome::Engine(error)) if error.is_internal() => internal(&anyhow::Error::new(error)),
        Ok(Outcome::Engine(error)) => engine(&error),
        Err(error) => internal(&error),
    }
}

/// The diagnostics of a project with errors, then the refusal.
fn engine(error: &EngineError) -> ExitCode {
    if let EngineError::Invalid(report) = error
        && let Err(print) = crate::check::print(report, false)
    {
        return internal(&print);
    }
    refuse(&error.to_string(), error.help().as_deref())
}

fn refuse(reason: &str, help: Option<&str>) -> ExitCode {
    eprintln!("error: {reason}");
    if let Some(help) = help {
        eprintln!("  = help: {help}");
    }
    ExitCode::from(2)
}

fn internal(error: &anyhow::Error) -> ExitCode {
    eprintln!("error: internal error in vc: {error:#}");
    eprintln!("  = help: this is a bug in Vibranium; please report it at {ISSUES}");
    ExitCode::from(70)
}
