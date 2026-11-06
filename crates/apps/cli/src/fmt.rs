//! `vc fmt`: rewrite definition files in the house style (D180).

use std::path::{Path, PathBuf};

use clap::Args;

use crate::{Outcome, changes, project};

#[derive(Args)]
pub struct FmtArgs {
    /// Removes the one-line comments `vc new` wrote above its items; yours stay.
    #[arg(long)]
    strip_comments: bool,
    /// Formats only the files at or under these paths; all definition files by default.
    paths: Vec<PathBuf>,
}

/// `vc fmt [--strip-comments] [paths…]`: exit 0 once written, 2 when a file does not parse or
/// cannot be formatted, and then nothing is written.
pub fn run(args: &FmtArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let paths = match located.project_paths(&args.paths) {
        Ok(paths) => paths,
        Err(reason) => return Ok(Outcome::Refused(reason)),
    };
    let made = located.engine.fmt(&paths, args.strip_comments);
    let count = made.as_ref().map_or(0, |changes| changes.files.len());
    let done = format!("✓ formatted {count} {}", if count == 1 { "file" } else { "files" });
    Ok(changes::write(&located.engine, made, &done))
}
