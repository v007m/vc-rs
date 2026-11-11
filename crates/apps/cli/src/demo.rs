//! `vc demo start|stop`: the small service the `vc init` sample calls (D175, D188). `start`
//! launches the `vc-demo` binary found next to `vc`, else on `PATH`; with `--detach` it runs in
//! the background, `vc` writes its pidfile and returns once it answers, and `stop` reads the
//! pidfile and ends it. A `start` that finds the demo running joins it instead, and the `stop`
//! after it leaves the demo to whoever else uses it.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use std::{env, fs, process, thread};

use clap::{Args, Subcommand};

use crate::Outcome;
use crate::stdout;

/// The port the sample's demo listens on.
const PORT: u16 = 7421;

/// The demo's binary, shipped next to `vc`.
const BINARY: &str = "vc-demo";

/// How to install `vc-demo` from the repo, until there are prebuilt installers.
const INSTALL: &str = "cargo install --git https://github.com/v007m/vc-rs vc-demo --locked";

/// How long `start --detach` waits for the demo to answer.
const WAIT: Duration = Duration::from_secs(10);

const POLL: Duration = Duration::from_millis(50);

/// How long [`is_demo`] waits for the program on a port to reply.
const PROBE: Duration = Duration::from_secs(2);

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
        return Ok(missing());
    };
    if is_demo(port) {
        join(port);
        writeln!(stdout::out(), "a demo already answers on port {port}")?;
        return Ok(Outcome::Done);
    }
    if TcpStream::connect_timeout(&local(port), POLL).is_ok() {
        let help = "pick another `--port`, and change `sampleUrl` in vibranium.config.yaml to match";
        return Ok(Outcome::Helped(
            format!("port {port} is taken by a program that is not the demo"),
            Some(help.into()),
        ));
    }
    fs::remove_dir_all(users(port)).ok();
    let mut command = Command::new(binary);
    command.arg("--port").arg(port.to_string());
    if !detach {
        return foreground(&mut command, port);
    }
    detached(&mut command);
    let mut child = command.spawn()?;
    fs::write(pidfile(port), child.id().to_string())?;
    up(&mut child, port, "`vc demo stop` stops it")
}

/// Runs the demo until it ends, saying where once it answers; exit 1 when it ends with an error.
fn foreground(command: &mut Command, port: u16) -> anyhow::Result<Outcome> {
    let mut child = command.spawn()?;
    let started = up(&mut child, port, "Ctrl-C stops it")?;
    if !matches!(started, Outcome::Done) {
        child.kill().ok();
        return Ok(started);
    }
    let code = child.wait()?.code().and_then(|code| u8::try_from(code).ok());
    Ok(code.map_or(Outcome::Exit(1), Outcome::Exit))
}

/// Waits for the demo to answer, then says where and how it stops (`stopping`); refuses when it
/// ends first or never answers.
fn up(child: &mut Child, port: u16, stopping: &str) -> anyhow::Result<Outcome> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if is_demo(port) {
            writeln!(stdout::out(), "demo running on http://localhost:{port} (pid {}); {stopping}", child.id())?;
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
    if left_to_another(port) {
        writeln!(stdout::out(), "left the demo on port {port} running, as another `vc demo start` uses it")?;
        return Ok(Outcome::Done);
    }
    let path = pidfile(port);
    let pid = fs::read_to_string(&path).ok().and_then(|text| text.trim().parse::<u32>().ok());
    let stopped = pid.is_some_and(terminate);
    fs::remove_file(&path).ok();
    let said = if stopped { "stopped the demo on port" } else { "no demo running on port" };
    writeln!(stdout::out(), "{said} {port}")?;
    Ok(Outcome::Done)
}

/// Exit 2: `vc-demo` is in neither place [`binary`] looks, named in the order it looks.
fn missing() -> Outcome {
    let folder = env::current_exe().ok().and_then(|exe| Some(exe.parent()?.display().to_string()));
    let beside = folder.map_or_else(String::new, |folder| format!(" (in {folder})"));
    let help = format!("install it with `{INSTALL}`, or put `{BINARY}` next to `vc`");
    Outcome::Helped(format!("`{BINARY}` is not next to vc{beside} or on PATH"), Some(help))
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

/// The folder with one file per `start` that joined the demo on `port`: each still uses it.
fn users(port: u16) -> PathBuf {
    env::temp_dir().join(format!("vibranium-demo-{port}.users"))
}

/// Notes that this `start` uses a demo another one launched; when it cannot, the next `stop`
/// ends the demo as before.
fn join(port: u16) {
    let folder = users(port);
    if fs::create_dir_all(&folder).is_ok() && own_folder(&folder) {
        fs::File::create_new(folder.join(process::id().to_string())).ok();
    }
}

/// Takes one joined `start` off the demo on `port`; whether there was one, so it keeps running.
fn left_to_another(port: u16) -> bool {
    let folder = users(port);
    let Some(entries) = own_folder(&folder).then(|| fs::read_dir(&folder).ok()).flatten() else { return false };
    let mut joined = entries.flatten().filter(|entry| entry.file_name().to_string_lossy().parse::<u32>().is_ok());
    joined.any(|entry| fs::remove_file(entry.path()).is_ok())
}

/// Whether `path` is a folder this user owns, not a link: the temp folder may be shared, and
/// another user's link there is never followed.
fn own_folder(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else { return false };
    #[cfg(unix)]
    let mine = std::os::unix::fs::MetadataExt::uid(&metadata) == rustix::process::getuid().as_raw();
    #[cfg(not(unix))]
    let mine = true;
    metadata.is_dir() && mine
}

fn local(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

/// Whether the demo answers on `port`: its `/echo` returns the path it was asked for, which a
/// program that only holds the port does not.
fn is_demo(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&local(port), POLL) else { return false };
    let mut reply = String::new();
    stream.set_read_timeout(Some(PROBE)).is_ok()
        && write!(stream, "GET /echo HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").is_ok()
        && stream.take(64 * 1024).read_to_string(&mut reply).is_ok()
        && reply.starts_with("HTTP/1.1 200")
        && reply.contains(r#""path":"/echo""#)
}

/// No terminal: the demo outlives `vc`, and a Ctrl-C to the shell does not reach it (its own
/// process group; on Windows also no console).
///
/// On Windows the demo still inherits every inheritable handle of `vc`, including the pipes a
/// caller reads `vc`'s output from: std has no stable way to stop that (`inherit_handles`,
/// rust-lang/rust#146407) and the workspace forbids `unsafe`. A caller that reads those pipes to
/// the end waits until `vc demo stop`; the `command:` action does not, it stops reading shortly
/// after `vc` exits.
fn detached(command: &mut Command) {
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(command, DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

/// `DETACHED_PROCESS`: the demo gets no console, so closing `vc`'s window leaves it running.
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// `CREATE_NEW_PROCESS_GROUP`: a Ctrl-C in `vc`'s console does not reach the demo.
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

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
