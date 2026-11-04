//! The `vc` binary (also installed as `vibranium`).

use std::panic;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod check;
mod list;
mod outcome;
mod plan;
mod project;
mod run;
mod schema;
mod select;

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
        Command::Schema(SchemaCommand::Export(args)) => schema::export(&args, project),
    }
}
