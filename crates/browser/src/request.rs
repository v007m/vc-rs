//! What the playground page asks for: the project's files and the command to run on them.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The command the page runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Command {
    /// `vc check`: the diagnostics and the summary line.
    Check,
    /// `vc run`: the spec report, against the demo service.
    #[default]
    Run,
}

/// One request from the page, read as JSON from stdin.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(clippy::exhaustive_structs, reason = "the page's JSON request; built with `..Default::default()` (D160)")]
pub struct Request {
    /// Each file by its project path, as the editor holds it.
    pub files: BTreeMap<String, String>,
    /// What to run.
    #[serde(default)]
    pub command: Command,
    /// Whether to colour the output with ANSI escapes, which the page renders.
    #[serde(default)]
    pub color: bool,
    /// The run's seed, as `--seed` gives it; a fresh one when not given.
    #[serde(default)]
    pub seed: Option<u64>,
}
