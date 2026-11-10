//! `vc approvals` and `vc approve <id>`: the agent calls waiting for a person (D195).

use std::io::Write;
use std::path::Path;

use clap::Args;
use vc_engine::{Approval, ApprovalId, ApprovalStatus, Engine, Method, Verdict};

use crate::stdout;
use crate::{Outcome, project};

/// `vc approvals`.
#[derive(Args)]
pub struct ApprovalsArgs {
    /// Also lists the requests already decided or expired.
    #[arg(long)]
    all: bool,
}

/// `vc approve <id>`.
#[derive(Args)]
pub struct ApproveArgs {
    /// The request to decide, as `vc approvals` lists it.
    id: u64,
    /// Also allows calls like it (same profile, tool and method) from now on (D195). A write,
    /// or a run that reaches a `command:` or `function:` action, still asks every time (D202).
    #[arg(long, conflicts_with = "deny")]
    always: bool,
    /// Refuses the call instead.
    #[arg(long)]
    deny: bool,
}

/// `vc approvals [--all]`: exit 0 listed, 2 the history cannot be read.
pub fn list(args: &ApprovalsArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    Ok(match located.engine.approvals_list(!args.all) {
        Ok(approvals) if approvals.is_empty() => {
            writeln!(stdout::out(), "{}", if args.all { "no requests" } else { "no requests waiting" })?;
            Outcome::Done
        }
        Ok(approvals) => {
            for approval in &approvals {
                writeln!(stdout::out(), "{}", row(approval, args.all))?;
            }
            Outcome::Done
        }
        Err(error) => Outcome::Engine(error),
    })
}

/// `vc approve <id> [--always | --deny]`: exit 0 decided, 2 unknown or already decided.
pub fn approve(args: &ApproveArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let verdict = match (args.always, args.deny) {
        (true, _) => Verdict::AlwaysAllow,
        (_, true) => Verdict::Deny,
        _ => Verdict::Approve,
    };
    let kept = if args.always { methods(&located.engine, args.id) } else { Vec::new() };
    Ok(match located.engine.approvals_decide(ApprovalId::new(args.id), verdict) {
        Ok(_) => {
            writeln!(stdout::out(), "{}", decided(args.id, verdict, &kept))?;
            Outcome::Done
        }
        Err(error) => Outcome::Engine(error),
    })
}

/// `#3  claude  call_api  staging  DELETE users#remove`, with the status when `all`.
fn row(approval: &Approval, all: bool) -> String {
    let request = &approval.request;
    let line =
        format!("#{}  {}  {}  {}  {}", approval.id, request.agent, request.tool, request.profile, request.summary);
    if all { format!("{line}  ({})", status(approval.status)) } else { line }
}

const fn status(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::Pending => "waiting",
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Denied => "denied",
        ApprovalStatus::Expired => "expired",
    }
}

/// The methods waiting request `id` asks for: what an always-allow of it keeps a rule for. None
/// when it is not waiting, which deciding it then reports.
fn methods(engine: &Engine, id: u64) -> Vec<Method> {
    let waiting = engine.approvals_list(true).unwrap_or_default();
    waiting
        .into_iter()
        .find(|approval| approval.id.get() == id)
        .map(|approval| approval.request.methods)
        .unwrap_or_default()
}

/// What deciding request `id` did; `kept`, the methods an always-allow kept a rule for. A request
/// that asked for no method asked for a write or for code, which no rule covers (D202).
fn decided(id: u64, verdict: Verdict, kept: &[Method]) -> String {
    match verdict {
        Verdict::Approve => format!("✓ approved request #{id}"),
        Verdict::AlwaysAllow if kept.is_empty() => {
            format!("✓ approved request #{id} once; no rule kept: a write or a run that reaches code asks every time")
        }
        Verdict::AlwaysAllow => {
            let methods = kept.iter().map(|method| method.name()).collect::<Vec<_>>().join(", ");
            format!("✓ approved request #{id}, and {methods} calls like it from now on; a write or code still asks")
        }
        Verdict::Deny => format!("✓ denied request #{id}"),
    }
}
