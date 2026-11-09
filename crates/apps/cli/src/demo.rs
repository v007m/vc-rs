//! `vc demo start|stop`: the small service the `vc init` sample calls (D175, D188). `start`
//! launches the `vc-demo` binary found next to `vc`, else on `PATH`; with `--detach` it runs in
//! the background, `vc` writes its pidfile and returns once it answers, and `stop` reads the
//! pidfile and ends it.

use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use std::{env, fs, thread};

use clap::{Args, Subcommand};

use crate::Outcome;

/// The port the sample's demo listens on.
const PORT: u16 = 7421;

/// The demo's binary, shipped next to `vc`.
const BINARY: &str = "vc-demo";

/// How long `start --detach` waits for the demo to answer.
const WAIT: Duration = Duration::from_secs(10);

const POLL: Duration = Duration::from_millis(50);

/// `vc demo …`.
#[derive(Args)]
pub struct DemoArgs {
    #[command(subcommand)]
    command: DemoCommand,
}

#[derive(Subcommand)]
enum DemoCommand {
    /// Starts the demo service (`/users`, `/echo`) on localhost.
    Start {
        /// The port to listen on.
        #[arg(long, default_value_t = PORT)]
        port: u16,
        /// Runs it in the background and returns once it answers; `vc demo stop` ends it.
        #[arg(long)]
        detach: bool,
    },
    /// Stops the demo `start --detach` started; stopping none is not an error.
    Stop {
        /// The port it listens on.
        #[arg(long, default_value_t = PORT)]
        port: u16,
    },
}

/// `vc demo start [--port] [--detach]` and `vc demo stop [--port]`: exit 0 done, 2 the demo
/// could not start.
pub fn run(args: &DemoArgs) -> anyhow::Result<Outcome> {
    match args.command {
        DemoCommand::Start { port, detach } => start(port, detach),
        DemoCommand::Stop { port } => stop(port),
    }
}

fn start(port: u16, detach: bool) -> anyhow::Result<Outcome> {
    let Some(binary) = binary() else {
        let help = format!("install `{BINARY}` next to `vc`; the installers ship both");
        return Ok(Outcome::Helped(format!("`{BINARY}` is not next to vc or on PATH"), Some(help)));
    };
    if answers(port) {
        writeln!(std::io::stdout().lock(), "a demo already answers on port {port}")?;
        return Ok(Outcome::Done);
    }
    let mut command = Command::new(binary);
    command.arg("--port").arg(port.to_string());
    if !detach {
        let code = command.status()?.code().and_then(|code| u8::try_from(code).ok());
        return Ok(code.map_or(Outcome::Exit(1), Outcome::Exit));
    }
    detached(&mut command);
    let mut child = command.spawn()?;
    fs::write(pidfile(port), child.id().to_string())?;
    up(&mut child, port)
}

/// Waits for the detached demo to answer; refuses when it stops first or never answers.
fn up(child: &mut Child, port: u16) -> anyhow::Result<Outcome> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if answers(port) {
            writeln!(std::io::stdout().lock(), "demo running on http://localhost:{port} (pid {})", child.id())?;
            return Ok(Outcome::Done);
        }
        if let Some(status) = child.try_wait()? {
            fs::remove_file(pidfile(port)).ok();
            return Ok(Outcome::Refused(format!("{BINARY} stopped before it answered on port {port} ({status})")));
        }
        thread::sleep(POLL);
    }
    Ok(Outcome::Refused(format!("{BINARY} did not answer on port {port} within {}s", WAIT.as_secs())))
}

fn stop(port: u16) -> anyhow::Result<Outcome> {
    let path = pidfile(port);
    let pid = fs::read_to_string(&path).ok().and_then(|text| text.trim().parse::<u32>().ok());
    let stopped = pid.is_some_and(terminate);
    fs::remove_file(&path).ok();
    let said = if stopped { "stopped the demo on port" } else { "no demo running on port" };
    writeln!(std::io::stdout().lock(), "{said} {port}")?;
    Ok(Outcome::Done)
}

/// `vc-demo` in the folder of the running `vc`, else the first on `PATH`.
fn binary() -> Option<PathBuf> {
    let name = format!("{BINARY}{}", env::consts::EXE_SUFFIX);
    let beside = env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join(&name)));
    let on_path = env::var_os("PATH").into_iter().flat_map(|path| env::split_paths(&path).collect::<Vec<_>>());
    beside.into_iter().chain(on_path.map(|dir| dir.join(&name))).find(|path| path.is_file())
}

/// The pidfile of the demo on `port`, in the temp folder so `stop` finds it from any folder.
fn pidfile(port: u16) -> PathBuf {
    env::temp_dir().join(format!("vibranium-demo-{port}.pid"))
}

fn answers(port: u16) -> bool {
    TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), POLL).is_ok()
}

/// No terminal: the demo outlives `vc`, and on Unix a Ctrl-C to the shell does not reach it.
fn detached(command: &mut Command) {
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
}

#[cfg(unix)]
fn terminate(pid: u32) -> bool {
    use rustix::process::{Pid, Signal, kill_process};
    i32::try_from(pid).ok().and_then(Pid::from_raw).is_some_and(|pid| kill_process(pid, Signal::TERM).is_ok())
}

#[cfg(not(unix))]
fn terminate(pid: u32) -> bool {
    let status = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).stdout(Stdio::null()).status();
    status.is_ok_and(|status| status.success())
}
