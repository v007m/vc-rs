//! The `vc` binary (also installed as `vibranium`).

use std::panic;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod approvals;
mod cache;
mod call;
mod changes;
mod check;
mod completion;
mod demo;
mod explain;
mod fix;
mod fmt;
mod history;
mod import;
mod init;
mod list;
mod load;
mod mcp;
mod migrate;
mod new;
mod outcome;
mod plan;
mod plugin;
mod project;
mod rename;
mod report;
mod run;
mod save_call;
mod schema;
mod select;
mod skip;
mod watch;

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
    /// Creates a project that runs green offline: config, sample apis and tests, folders.
    Init(init::InitArgs),
    /// Compiles and links every definition file and prints its errors and warnings.
    Check(check::CheckArgs),
    /// Shows what a run would do, in order, without sending a request.
    Plan(plan::PlanCommandArgs),
    /// Runs the selected tests.
    Run(run::RunArgs),
    /// Runs the selected tests, then again whenever a file they read is saved.
    Watch(run::RunArgs),
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
    /// Renames an api, test, file, var, auth or dataset ID and every reference to it.
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
    /// Explains a diagnostic code, or a test or api as the active profile resolves it.
    Explain(explain::ExplainArgs),
    /// Shows where a config value comes from.
    #[command(subcommand)]
    Config(explain::ConfigCommand),
    /// Sends one api ad hoc and prints the response.
    Call(call::CallArgs),
    /// Creates an api file from an `OpenAPI` spec, a HAR recording or another tool's collection.
    Import(import::ImportArgs),
    /// Merges spec changes into api files, keeping your edits.
    Sync(import::SyncArgs),
    /// Lists the agent calls waiting for a person's approval.
    Approvals(approvals::ApprovalsArgs),
    /// Approves or denies an agent call that is waiting.
    Approve(approvals::ApproveArgs),
    /// Runs a load scenario with live metrics.
    Load(load::LoadArgs),
    /// Converts a project from an older format.
    #[command(subcommand)]
    Migrate(migrate::MigrateCommand),
    /// Manages plugins and their permissions.
    Plugin(plugin::PluginArgs),
    /// Starts the MCP server for AI agents, over stdio.
    Mcp(mcp::McpArgs),
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
        Command::Init(args) => init::run(&args, project),
        Command::Check(args) => check::run(&args, project),
        Command::Plan(args) => plan::run(&args, project),
        Command::Run(args) => run::run(&args, project),
        Command::Watch(args) => watch::run(&args, project),
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
        Command::Explain(args) => explain::run(&args, project),
        Command::Config(command) => explain::config(&command, project),
        Command::Call(args) => call::run(&args, project),
        Command::Import(args) => import::import(&args, project),
        Command::Sync(args) => import::sync(&args, project),
        Command::Approvals(args) => approvals::list(&args, project),
        Command::Approve(args) => approvals::approve(&args, project),
        Command::Load(args) => load::run(&args, project),
        Command::Migrate(command) => migrate::run(&command, project),
        Command::Plugin(args) => plugin::run(&args, project),
        Command::Mcp(args) => mcp::run(&args, project),
        Command::Demo(args) => demo::run(&args),
        Command::Completion(args) => completion::run(&args),
        Command::Complete(args) => completion::complete(&args, project),
    }
}
