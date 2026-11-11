//! `vibranium`, the long name of `vc` (D39, D44). It runs the `vc` installed beside it with the
//! same arguments and exits with its code, so the long name works where another tool owns `vc`.

use std::env;
use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

/// How to install both names.
const INSTALL: &str = "cargo install --git https://github.com/v007m/vc-rs vc-cli vc-demo --locked";

fn main() -> ExitCode {
    let mut vc = Command::new(vc());
    vc.args(env::args_os().skip(1));
    run(&mut vc)
}

/// The `vc` in this binary's folder, else the first on `PATH`.
fn vc() -> PathBuf {
    let name = format!("vc{}", env::consts::EXE_SUFFIX);
    let beside = env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join(&name)));
    beside.filter(|path| path.is_file()).unwrap_or_else(|| PathBuf::from(name))
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

/// Exit 2: `vc` is not installed beside `vibranium` or on `PATH`.
fn failed(error: &io::Error) -> ExitCode {
    eprintln!("error: `vibranium` runs `vc`, which is not next to it or on PATH: {error}");
    eprintln!("  = help: install both with `{INSTALL}`");
    ExitCode::from(2)
}
