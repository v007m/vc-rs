//! Writes a generated project to disk, to look at or to run `vc check` on:
//!
//! ```text
//! cargo run -p vc-testkit --example generate_project -- <dir> <files> [seed]
//! ```

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(dir), Some(Ok(files))) = (args.first(), args.get(1).map(|n| n.parse::<usize>())) else {
        eprintln!("usage: generate_project <dir> <files> [seed]");
        return ExitCode::from(2);
    };
    let seed = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(1);
    for file in vc_testkit::generate_project(seed, files).files() {
        if let Err(error) = write(&Path::new(dir).join(&file.path), &file.text) {
            eprintln!("{}: {error}", file.path);
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)
}
