//! The `vc` binary (also installed as `vibranium`).

use std::panic;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod cache;
mod call;
mod check;
mod completion;
mod demo;
mod explain;
mod history;
mod list;
mod outcome;
mod plan;
mod project;
mod report;
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
    /// Lists the stored runs, shows one, or drops the oldest.
    History(history::HistoryArgs),
    /// Writes the report files of a stored run again.
    Report(report::ReportArgs),
    /// Lists or clears the cross-run cache of `cache:` actions.
    Cache(cache::CacheArgs),
    /// Works with the JSON Schemas editors use for definition files.
    #[command(subcommand)]
    Schema(SchemaCommand),
    /// Explains a diagnostic code, or a test or api as the active profile resolves it.
    Explain(explain::ExplainArgs),
    /// Shows where a config value comes from.
    #[command(subcommand)]
    Config(explain::ConfigCommand),
    /// Sends one api ad hoc and prints the response.
    Call(call::CallArgs),
    /// Starts or stops the demo service the `vc init` sample calls.
    Demo(demo::DemoArgs),
    /// Prints the shell completion script for bash, zsh, fish or powershell.
    Completion(completion::CompletionArgs),
    /// The test and api IDs, profiles and tags the completion scripts offer.
    #[command(name = "__complete", hide = true)]
    Complete(completion::CompleteArgs),
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
        Command::Schema(SchemaCommand::Export(args)) => schema::export(&args, project),
        Command::Explain(args) => explain::run(&args, project),
        Command::Config(command) => explain::config(&command, project),
        Command::Call(args) => call::run(&args, project),
        Command::Demo(args) => demo::run(&args),
        Command::Completion(args) => completion::run(&args),
        Command::Complete(args) => completion::complete(&args, project),
    }
}
