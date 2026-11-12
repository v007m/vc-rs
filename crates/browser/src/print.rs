//! What the CLI prints around a command, word for word (`vc-cli`'s `outcome.rs`, `check.rs` and
//! `run.rs`): the diagnostics and their summary, warnings, refusals and the seed of a failed run.

use vc_diagnostics::{Render as _, TextRenderer};

use crate::error::Checked;
use crate::{BrowserError, Sink, Stream};

/// Where the CLI asks for bug reports.
const ISSUES: &str = "https://github.com/v007m/vc-rs/issues";

/// Prints the diagnostics of `checked`, as `vc check` does, then its summary line.
pub fn diagnostics(sink: &dyn Sink, checked: &Checked, color: bool) -> Result<(), BrowserError> {
    let renderer = TextRenderer::default().color(color);
    let text = renderer.render(&checked.diagnostics, &checked.files).map_err(internal)?;
    sink.print(Stream::Out, &text);
    sink.line(Stream::Out, &checked.diagnostics.summary().line(checked.files.len()));
    Ok(())
}

/// Prints why `error` stopped the command, and returns its exit code.
pub fn error(sink: &dyn Sink, error: &BrowserError, color: bool) -> u8 {
    match error {
        BrowserError::Invalid(checked) => {
            if let Err(printing) = diagnostics(sink, checked, color) {
                return self::error(sink, &printing, color);
            }
            sink.line(Stream::Err, &format!("error: {error}"));
        }
        BrowserError::Refused { reason, help } => {
            sink.line(Stream::Err, &labelled("error", reason));
            if let Some(help) = help {
                sink.line(Stream::Err, &format!("  = help: {help}"));
            }
        }
        BrowserError::Internal { .. } => {
            sink.line(Stream::Err, &format!("error: {error}"));
            sink.line(Stream::Err, &format!("  = help: this is a bug in Vibranium; please report it at {ISSUES}"));
        }
    }
    error.exit()
}

/// A run's notice as the CLI warns it: `warning[V5118]: …` when it starts with a code.
pub fn warning(notice: &str) -> String {
    labelled("warning", notice)
}

/// `text` under `level`, as `level[V5118]: …` when it starts with a code.
fn labelled(level: &str, text: &str) -> String {
    let coded = text.split_once(": ").filter(|(code, _)| {
        code.len() == 5 && code.starts_with('V') && code.bytes().skip(1).all(|byte| byte.is_ascii_digit())
    });
    coded.map_or_else(|| format!("{level}: {text}"), |(code, rest)| format!("{level}[{code}]: {rest}"))
}

/// Prints a run's notices, and the seed of a failed run.
pub fn notes(sink: &dyn Sink, notices: &[String], (exit, seed): (u8, u64)) {
    for notice in notices {
        sink.line(Stream::Err, &warning(notice));
    }
    if exit == 1 {
        sink.line(Stream::Err, &format!("seed {seed}: pass `--seed {seed}` to run with the same generated data"));
    }
}

pub fn internal(error: impl std::fmt::Display) -> BrowserError {
    BrowserError::Internal { reason: error.to_string() }
}

#[cfg(test)]
mod tests {
    use super::warning;

    #[test]
    fn a_coded_notice_is_a_coded_warning() {
        assert_eq!(warning("V5118: the report failed"), "warning[V5118]: the report failed");
        assert_eq!(warning("history: off"), "warning: history: off");
        assert_eq!(warning("V51x8: no"), "warning: V51x8: no");
    }
}
