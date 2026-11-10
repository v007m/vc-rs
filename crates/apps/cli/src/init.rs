//! `vc init`: a new project in the working folder, or in `--project <dir>` (D89, D175).

use std::io::Write;
use std::path::Path;

use clap::Args;
use vc_engine::{EngineBuilder, INIT_FOLDERS};

use crate::Outcome;
use crate::stdout;

#[derive(Args)]
pub struct InitArgs {
    /// Writes the config and folders only, without the sample api and test files.
    #[arg(long)]
    no_sample: bool,
    /// Then imports this `OpenAPI` spec, a URL or a path in the project, as `vc import openapi`.
    #[arg(long, value_name = "URL_OR_FILE")]
    from_openapi: Option<String>,
}

/// `vc init [--no-sample] [--from-openapi <url-or-file>]`: exit 0 once written, 2 when a
/// project is here already, and then nothing is written. The import runs once the project is
/// written; when it is refused, the project stays and `vc import openapi` can be run again.
pub fn run(args: &InitArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let cwd = std::env::current_dir()?;
    let root = project.map_or_else(|| cwd.clone(), |dir| cwd.join(dir));
    std::fs::create_dir_all(&root)?;
    let engine = match EngineBuilder::new(root.clone()).build() {
        Ok(engine) => engine,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    let changes = match engine.init(!args.no_sample) {
        Ok(changes) => changes,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    if let Err(error) = engine.write(&changes) {
        return Ok(Outcome::Engine(error));
    }
    for folder in INIT_FOLDERS {
        std::fs::create_dir_all(root.join(folder))?;
    }
    writeln!(stdout::out(), "✓ created a Vibranium project in {}", root.display())?;
    for file in &changes.files {
        writeln!(stdout::out(), "  {}", file.path)?;
    }
    writeln!(stdout::out(), "  {}/", INIT_FOLDERS.join("/  "))?;
    if let Some(source) = &args.from_openapi {
        return crate::import::openapi(project, source);
    }
    writeln!(stdout::out(), "next: {}", if args.no_sample { "vc new api <service>#<id> --path /…" } else { "vc run" })?;
    Ok(Outcome::Done)
}
