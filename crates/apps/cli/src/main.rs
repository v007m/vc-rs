//! The `vc` binary (also installed as `vibranium`).

use std::panic;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod cache;
mod changes;
mod check;
mod fix;
mod fmt;
mod history;
mod list;
mod new;
mod outcome;
mod plan;
mod project;
mod rename;
mod report;
mod run;
mod schema;
mod select;
mod skip;

use outcome::Outcome;

/// Vibranium: functional, contract and load tests for HTTP APIs.
#[derive(Parser)]
#[command(name = "vc", version, about, arg_required_else_help = true)]
struct Cli {
    /// The project to work on: this folder or the nearest one above it with a
    /// `vibranium.config.yaml`. Relative paths and selectors then start at the project root.
    #[arg(long, global = true, value_name = "DIR")]
    project: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compiles and links every definition file and prints its errors and warnings.
    Check(check::CheckArgs),
    /// Shows what a run would do, in order, without sending a request.
    Plan(plan::PlanCommandArgs),
    /// Runs the selected tests.
    Run(run::RunArgs),
    /// Lists the tests, apis or definition files.
    List(list::ListArgs),
    /// Lists the stored runs, shows one, or drops the oldest.
    History(history::HistoryArgs),
    /// Writes the report files of a stored run again.
    Report(report::ReportArgs),
    /// Lists or clears the cross-run cache of `cache:` actions.
    Cache(cache::CacheArgs),
    /// Adds an api, test or auth definition, or creates an empty definition file.
    New(new::NewArgs),
    /// Renames an api, test or file ID and every reference to it.
    Rename(rename::RenameArgs),
    /// Rewrites definition files in the house style, keeping their comments.
    Fmt(fmt::FmtArgs),
    /// Skips a test or test file, with a reason and an optional end date.
    Skip(skip::SkipArgs),
    /// Removes the skip of a test or test file.
    Unskip(skip::UnskipArgs),
    /// Works with the JSON Schemas editors use for definition files.
    #[command(subcommand)]
    Schema(SchemaCommand),
}

#[derive(Subcommand)]
enum SchemaCommand {
    /// Writes the JSON Schema of each kind of definition file.
    Export(schema::ExportArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = panic::catch_unwind(AssertUnwindSafe(|| run(cli)));
    outcome::finish(result.unwrap_or_else(|_| Err(anyhow::anyhow!("vc stopped unexpectedly"))))
}

fn run(cli: Cli) -> anyhow::Result<Outcome> {
    let project = cli.project.as_deref();
    match cli.command {
        Command::Check(args) => check::run(&args, project),
        Command::Plan(args) => plan::run(&args, project),
        Command::Run(args) => run::run(&args, project),
        Command::List(args) => list::run(&args, project),
        Command::History(args) => history::run(&args, project),
        Command::Report(args) => report::run(&args, project),
        Command::Cache(args) => cache::run(&args, project),
        Command::New(args) => new::run(args, project),
        Command::Rename(args) => rename::run(&args, project),
        Command::Fmt(args) => fmt::run(&args, project),
        Command::Skip(args) => skip::skip(&args, project),
        Command::Unskip(args) => skip::unskip(&args, project),
        Command::Schema(SchemaCommand::Export(args)) => schema::export(&args, project),
    }
}
