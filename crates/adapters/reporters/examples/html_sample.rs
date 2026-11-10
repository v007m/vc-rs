//! Writes the HTML report of the synthetic run the report budgets use, for the web UI's report
//! spec (D169: first paint under a second, a test opened under 100 ms):
//!
//! `cargo run -q -p vc-adapter-reporters --example html_sample -- <out-dir> <tests> <folder|single>`

use std::error::Error;
use std::path::PathBuf;

use vc_adapter_reporters::{FormatRegistry, FsDestination, HtmlLayout, sample_snapshot};

/// How to run it.
const USAGE: &str = "usage: html_sample <out-dir> <tests> <folder|single>";

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(out), Some(tests), Some(layout)) = (args.next(), args.next(), args.next()) else {
        return Err(USAGE.into());
    };
    let layout = match layout.as_str() {
        "folder" => HtmlLayout::Folder,
        "single" => HtmlLayout::Single,
        _ => return Err(USAGE.into()),
    };
    let format = FormatRegistry::builtin(layout).get("html")?;
    let artifacts = format.render(&sample_snapshot(tests.parse()?), &FsDestination::new(PathBuf::from(out)))?;
    println!("wrote {} file(s)", artifacts.len());
    Ok(())
}
