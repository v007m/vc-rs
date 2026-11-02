//! The `vc` binary (also installed as `vibranium`).

use clap::Parser;

/// Vibranium: functional, contract and load tests for HTTP APIs.
#[derive(Parser)]
#[command(name = "vc", version, about, arg_required_else_help = true)]
struct Cli {}

fn main() {
    Cli::parse();
}
