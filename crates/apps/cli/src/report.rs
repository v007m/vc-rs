use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Args;
use vc_engine::{ReportFlags, ReportRequest};

use crate::Outcome;
use crate::project;
use crate::stdout;

#[derive(Args)]
pub struct ReportArgs {
    /// `last` or a run number from `vc history`.
    #[arg(default_value = "last")]
    run: String,
    /// The report files to write, as a comma list or repeated: `html`, `ctrf`, `junit`; `html`
    /// when not given.
    #[arg(long = "format", value_name = "FORMATS")]
    formats: Vec<String>,
    /// The folder the files go in, over `.vibranium/reports/<run>/`.
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
    /// Writes the HTML report as one self-contained file.
    #[arg(long)]
    single_file: bool,
}

/// `vc report [run] [--format ...] [--out DIR] [--single-file]`: writes a stored run's report
/// files again from its snapshot.
pub fn run(args: &ReportArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let request = ReportRequest {
        run: args.run.clone(),
        files: ReportFlags {
            formats: args.formats.clone(),
            out: args.out.as_ref().map(|out| std::env::current_dir().map(|cwd| cwd.join(out))).transpose()?,
            single_file: args.single_file,
        },
    };
    let written = match located.engine.report(&request) {
        Ok(written) => written,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    for notice in &written.notices {
        eprintln!("warning: {notice}");
    }
    let mut out = stdout::out();
    for file in &written.files {
        writeln!(out, "{}", file.display())?;
    }
    Ok(if written.files.is_empty() { Outcome::Failed } else { Outcome::Done })
}
