use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::Args;
use vc_engine::{CheckReport, CheckRequest, JsonRenderer, Render, TextRenderer};

use crate::Outcome;
use crate::project;
use crate::stdout;

#[derive(Args)]
pub struct CheckArgs {
    /// Prints the diagnostics as JSON on stdout, for scripts, editors and agents.
    #[arg(long, conflicts_with = "fix")]
    json: bool,
    /// Shows the certain fixes as a diff and applies them when you confirm.
    #[arg(long)]
    fix: bool,
    /// Reports only on files at or under these paths; the whole project is compiled either way.
    paths: Vec<PathBuf>,
}

/// `vc check [--json | --fix] [paths…]`: exit 0 without errors, 2 with errors or when it cannot
/// start.
pub fn run(args: &CheckArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let paths = match located.project_paths(&args.paths) {
        Ok(paths) => paths,
        Err(reason) => return Ok(Outcome::Refused(reason)),
    };
    if args.fix {
        return crate::fix::run(&located.engine, &CheckRequest { paths, ..CheckRequest::default() });
    }
    let report = match located.engine.check(&CheckRequest { paths, ..CheckRequest::default() }) {
        Ok(report) => report,
        Err(error) => return Ok(Outcome::Refused(error.to_string())),
    };
    for notice in &report.notices {
        eprintln!("warning: {notice}");
    }
    print(&report, args.json)?;
    Ok(outcome(&report))
}

/// Exit 2 when the report has errors, else 0.
pub fn outcome(report: &CheckReport) -> Outcome {
    if report.has_errors() { Outcome::Failed } else { Outcome::Done }
}

/// The diagnostics on stdout, as text with the summary line or as JSON.
pub fn print(report: &CheckReport, json: bool) -> anyhow::Result<()> {
    let mut out = stdout::out();
    if json {
        writeln!(out, "{}", JsonRenderer.render(&report.diagnostics, &report.files)?)?;
    } else {
        let renderer = TextRenderer::default().color(use_color(&std::io::stdout()));
        write!(out, "{}", renderer.render(&report.diagnostics, &report.files)?)?;
        writeln!(out, "{}", report.summary().line(report.checked))?;
    }
    Ok(())
}

/// Colour only on a terminal, and never when `NO_COLOR` is set to something (no-color.org).
pub fn use_color(out: &impl IsTerminal) -> bool {
    out.is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}
