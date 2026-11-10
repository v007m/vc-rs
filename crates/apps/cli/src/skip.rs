//! `vc skip` and `vc unskip`: write or remove a `skip` in the YAML (D77).

use std::path::Path;

use clap::Args;
use vc_engine::SkipRequest;

use crate::{Outcome, changes, project};

#[derive(Args)]
pub struct SkipArgs {
    /// The test (`users.crud#listsUsers`) or test file (`users.crud`) to skip.
    target: String,
    /// Why, for the report.
    #[arg(long)]
    reason: String,
    /// The last day of the skip, `YYYY-MM-DD` (UTC); it runs again after it.
    #[arg(long, value_name = "DATE")]
    until: Option<String>,
}

#[derive(Args)]
pub struct UnskipArgs {
    /// The test or test file to run again.
    target: String,
}

/// `vc skip <id> --reason <text> [--until <date>]`: exit 0 once written, 2 when refused.
pub fn skip(args: &SkipArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let request = SkipRequest { target: args.target.clone(), reason: args.reason.clone(), until: args.until.clone() };
    let until = args.until.as_deref().map(|until| format!(" until {until}")).unwrap_or_default();
    let done = format!("✓ skipped {}{until}: {}", args.target, args.reason.trim());
    Ok(changes::write(&located.engine, located.engine.skip(&request), &done))
}

/// `vc unskip <id>`: exit 0 once removed, 2 when refused.
pub fn unskip(args: &UnskipArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let done = format!("✓ unskipped {}", args.target);
    Ok(changes::write(&located.engine, located.engine.unskip(&args.target), &done))
}
