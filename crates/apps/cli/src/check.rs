use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::Args;
use vc_engine::{CheckReport, CheckRequest, Engine, EngineBuilder, JsonRenderer, Render, Summary, TextRenderer};

use crate::Outcome;

#[derive(Args)]
pub struct CheckArgs {
    /// Prints the diagnostics as JSON on stdout, for scripts, editors and agents.
    #[arg(long)]
    json: bool,
    /// Reports only on files at or under these paths; the whole project is compiled either way.
    paths: Vec<PathBuf>,
}

/// `vc check [--json] [paths…]`: exit 0 without errors, 2 with errors or when it cannot start.
pub fn run(args: &CheckArgs) -> anyhow::Result<Outcome> {
    let cwd = std::env::current_dir()?;
    let engine = match EngineBuilder::for_project(&cwd).and_then(EngineBuilder::build) {
        Ok(engine) => engine,
        Err(error) => return Ok(Outcome::Refused(error.to_string())),
    };
    let paths = match project_paths(&engine, &cwd, &args.paths) {
        Ok(paths) => paths,
        Err(reason) => return Ok(Outcome::Refused(reason)),
    };
    let report = match engine.check(&CheckRequest { paths }) {
        Ok(report) => report,
        Err(error) => return Ok(Outcome::Refused(error.to_string())),
    };
    for notice in &report.notices {
        eprintln!("warning: {notice}");
    }
    print(&report, args.json)?;
    Ok(if report.has_errors() { Outcome::Failed } else { Outcome::Done })
}

fn print(report: &CheckReport, json: bool) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    if json {
        writeln!(out, "{}", JsonRenderer.render(&report.diagnostics, &report.files)?)?;
    } else {
        let renderer = TextRenderer::default().color(use_color(&out));
        write!(out, "{}", renderer.render(&report.diagnostics, &report.files)?)?;
        writeln!(out, "{}", summary_line(report.summary(), report.checked))?;
    }
    Ok(())
}

/// Colour only on a terminal, and never when `NO_COLOR` is set to something (no-color.org).
fn use_color(out: &impl IsTerminal) -> bool {
    out.is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// `1 error · 0 warnings · 3 files checked`.
fn summary_line(summary: Summary, checked: usize) -> String {
    let counts = [(summary.errors, "error"), (summary.warnings, "warning"), (checked, "file")];
    let [errors, warnings, files] =
        counts.map(|(count, noun)| format!("{count} {noun}{}", if count == 1 { "" } else { "s" }));
    format!("{errors} · {warnings} · {files} checked")
}

/// Each path relative to the project root with `/` separators, resolved from `cwd`.
fn project_paths(engine: &Engine, cwd: &Path, paths: &[PathBuf]) -> Result<Vec<String>, String> {
    let missing =
        |path: &Path| format!("{}: not a file or folder in the project at {}", path.display(), engine.root().display());
    paths.iter().map(|path| project_path(engine.root(), &cwd.join(path)).ok_or_else(|| missing(path))).collect()
}

fn project_path(root: &Path, path: &Path) -> Option<String> {
    let resolved = std::fs::canonicalize(path).ok()?;
    let relative = resolved.strip_prefix(root).ok()?;
    let parts: Vec<_> = relative.components().map(|part| part.as_os_str().to_string_lossy()).collect();
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use vc_engine::Summary;

    use super::summary_line;

    #[test]
    fn the_summary_line_counts_with_plurals() {
        assert_eq!(summary_line(Summary::default(), 3), "0 errors · 0 warnings · 3 files checked");
        assert_eq!(
            summary_line(Summary { errors: 1, warnings: 1, infos: 0 }, 1),
            "1 error · 1 warning · 1 file checked"
        );
    }
}
