//! The `vc` binary (also installed as `vibranium`).

use std::panic;
use std::panic::AssertUnwindSafe;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod check;
mod outcome;
mod schema;

use outcome::Outcome;

/// Vibranium: functional, contract and load tests for HTTP APIs.
#[derive(Parser)]
#[command(name = "vc", version, about, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compiles and links every definition file and prints its errors and warnings.
    Check(check::CheckArgs),
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
    match cli.command {
        Command::Check(args) => check::run(&args),
        Command::Schema(SchemaCommand::Export(args)) => schema::export(&args),
    }
}
