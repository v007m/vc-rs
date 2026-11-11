//! `vibranium`, the long name of `vc` (D39, D44). It runs the `vc` installed beside it with the
//! same arguments and exits with its code, so the long name works where another tool owns `vc`.
//! It never looks on `PATH`, so it cannot run another tool's `vc`.

use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::{env, fs};

/// How to install both names.
const INSTALL: &str = "cargo install --git https://github.com/v007m/vc-rs vc-cli vc-demo --locked";

fn main() -> ExitCode {
    match vc() {
        Ok(path) => run(Command::new(path).args(env::args_os().skip(1))),
        Err(error) => failed(&error),
    }
}

/// The `vc` in the folder of this binary's real file, after any symlinks.
fn vc() -> io::Result<PathBuf> {
    let name = format!("vc{}", env::consts::EXE_SUFFIX);
    let exe = fs::canonicalize(env::current_exe()?)?;
    let folder = exe.parent().ok_or_else(|| io::Error::other(format!("{} has no folder", exe.display())))?;
    let beside = folder.join(&name);
    if beside.is_file() {
        return Ok(beside);
    }
    Err(io::Error::new(io::ErrorKind::NotFound, format!("there is no `{name}` in {}", folder.display())))
}

/// Replaces this process with `vc`; returns only when it cannot start.
#[cfg(unix)]
fn run(vc: &mut Command) -> ExitCode {
    use std::os::unix::process::CommandExt;
    failed(&vc.exec())
}

/// Runs `vc` and exits with its code.
#[cfg(not(unix))]
fn run(vc: &mut Command) -> ExitCode {
    match vc.status() {
        Ok(status) => status.code().and_then(|code| u8::try_from(code).ok()).map_or(ExitCode::from(70), ExitCode::from),
        Err(error) => failed(&error),
    }
}

/// Exit 2: `vc` is not installed beside `vibranium`.
fn failed(error: &io::Error) -> ExitCode {
    eprintln!("error: `vibranium` runs the `vc` next to it, and cannot: {error}");
    eprintln!("  = help: install both with `{INSTALL}`");
    ExitCode::from(2)
}
