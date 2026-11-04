use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Args;
use vc_engine::Engine;

use crate::Outcome;
use crate::project;

#[derive(Args)]
pub struct ExportArgs {
    /// Writes one `<kind>.schema.json` per kind into this folder, creating it if needed,
    /// instead of printing every schema as one JSON object keyed by kind.
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
}

/// `vc schema export [--out <dir>]`; with `--project`, a relative `--out` is under the
/// project root.
pub fn export(args: &ExportArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let Some(out) = &args.out else {
        std::io::stdout().lock().write_all(Engine::schema_bundle()?.as_bytes())?;
        return Ok(Outcome::Done);
    };
    let dir = match project.map(project::root).transpose()? {
        Some(Ok(root)) => root.join(out),
        Some(Err(outcome)) => return Ok(outcome),
        None => out.clone(),
    };
    if let Err(error) = std::fs::create_dir_all(&dir) {
        return Ok(Outcome::Refused(format!("{}: {error}", dir.display())));
    }
    let files = Engine::schema_export()?;
    for file in &files {
        let path = dir.join(&file.file_name);
        if let Err(error) = std::fs::write(&path, &file.text) {
            return Ok(Outcome::Refused(format!("{}: {error}", path.display())));
        }
    }
    eprintln!("wrote {} schemas to {}", files.len(), dir.display());
    Ok(Outcome::Done)
}
