//! Why the playground did not run: the CLI's three kinds of refusal.

use vc_diagnostics::Diagnostics;
use vc_model::SourceFiles;

/// The project's diagnostics and the files they point into.
#[derive(Clone, Debug, Default)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a plain report the playground builds and prints; every user is in this repo (D160)"
)]
pub struct Checked {
    /// Every diagnostic, sorted.
    pub diagnostics: Diagnostics,
    /// The files the diagnostics point into.
    pub files: SourceFiles,
}

/// Why a command did not run, as the CLI words it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BrowserError {
    /// The project has errors: its diagnostics are printed, then this.
    #[error("the project has {} error(s); nothing ran", .0.diagnostics.summary().errors)]
    Invalid(Box<Checked>),
    /// A command the playground cannot carry out, or a selection that matches nothing.
    #[error("{reason}")]
    Refused {
        /// What was refused.
        reason: String,
        /// What to do instead.
        help: Option<String>,
    },
    /// A bug in Vibranium.
    #[error("internal error in vc: {reason}")]
    Internal {
        /// What went wrong.
        reason: String,
    },
}

impl BrowserError {
    /// The error that carries `diagnostics`, sorted, over `files`.
    #[must_use]
    pub fn invalid(mut diagnostics: Diagnostics, files: SourceFiles) -> Self {
        diagnostics.sort();
        Self::Invalid(Box::new(Checked { diagnostics, files }))
    }

    /// A refusal with `help`.
    pub fn refused(reason: impl Into<String>, help: Option<String>) -> Self {
        Self::Refused { reason: reason.into(), help }
    }

    /// The exit code the CLI gives this error: 70 for a bug, else 2.
    #[must_use]
    pub const fn exit(&self) -> u8 {
        if matches!(self, Self::Internal { .. }) { 70 } else { 2 }
    }
}
