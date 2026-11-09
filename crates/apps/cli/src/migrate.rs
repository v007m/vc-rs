//! `vc migrate v1 <path>`: converts a Vibranium v1 project into v2 files (D194, D200).

use std::path::PathBuf;

use clap::Subcommand;

use crate::Outcome;

/// `vc migrate …`.
#[derive(Subcommand)]
pub enum MigrateCommand {
    /// Converts the Vibranium v1 project at a path into v2 definition files.
    V1 {
        /// The v1 project folder.
        path: PathBuf,
    },
}

/// `vc migrate v1 <path>`: exit 2 until the converter is built (M7).
pub fn run(command: &MigrateCommand) -> Outcome {
    let MigrateCommand::V1 { path } = command;
    Outcome::Refused(format!("vc migrate v1 {} is not built yet", path.display()))
}
