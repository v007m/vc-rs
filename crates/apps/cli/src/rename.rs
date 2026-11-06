//! `vc rename <old> <new>`: an api, test or file ID and every reference to it (D87).

use std::path::Path;

use clap::Args;

use crate::{Outcome, changes, project};

#[derive(Args)]
pub struct RenameArgs {
    /// The api or test (`users#create`), or the file (`users.crud`), to rename.
    old: String,
    /// Its new ID: an item keeps its file (`users#createUser`); a new file ID moves the file.
    new: String,
}

/// `vc rename <old> <new>`: exit 0 once written, 2 when refused.
pub fn run(args: &RenameArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let done = format!("✓ renamed {} to {}", args.old, args.new);
    Ok(changes::write(&located.engine, located.engine.rename(&args.old, &args.new), &done))
}
