use std::io::Write;
use std::path::PathBuf;

use clap::Args;
use vc_engine::Engine;

use crate::Outcome;

#[derive(Args)]
pub struct ExportArgs {
    /// Writes one `<kind>.schema.json` per kind into this folder, creating it if needed,
    /// instead of printing every schema as one JSON object keyed by kind.
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
}

/// `vc schema export [--out <dir>]`.
pub fn export(args: &ExportArgs) -> anyhow::Result<Outcome> {
    let Some(dir) = &args.out else {
        std::io::stdout().lock().write_all(Engine::schema_bundle()?.as_bytes())?;
        return Ok(Outcome::Done);
    };
    if let Err(error) = std::fs::create_dir_all(dir) {
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
