//! Running programs (D163, D181): arguments as a list, never through a shell of its own.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::pin::{Pin, pin};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt as _};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use vc_interfaces::{CommandError, CommandOutput, CommandRunner, CommandSpec};

/// How much is read from a stream at a time.
const CHUNK: usize = 8 * 1024;
/// How long the streams are still read once the program has exited: a process it left running
/// in the background may hold them open.
const DRAIN: Duration = Duration::from_millis(200);

/// A [`CommandRunner`] that starts the program itself, with stdin closed.
///
/// It reads stdout and stderr as they come and keeps the last `output_cap` bytes of each, so a
/// chatty program never fills memory. The run ends when the program exits; what its streams
/// still hold is read for [`DRAIN`] more, so a process it left in the background never holds
/// the run open. That process lives on while vc runs, a detached task reading and discarding
/// what it writes; once vc exits, it dies of `SIGPIPE` at its next write. On unix the program leads a process group
/// of its own, and a timeout, or dropping the run, kills the whole group; elsewhere it kills the
/// program. The folder of the running `vc` goes on the end of the program's `PATH` when it is not
/// there yet, so a line calling `vc` finds it though `vc` was started by its path; a `vc` already
/// on `PATH` is still the one found.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct ProcessRunner;

impl ProcessRunner {
    /// A runner.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CommandRunner for ProcessRunner {
    async fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, CommandError> {
        let mut child = command(spec).spawn().map_err(|error| not_run(spec, &error))?;
        let mut group = Group { leader: child.id(), running: true };
        let collected = tokio::time::timeout(spec.timeout, collect(&mut child, spec.output_cap))
            .await
            .map_err(|_| CommandError::TimedOut { program: spec.program.clone(), after: spec.timeout })?;
        group.running = false;
        collected.map_err(|error| not_run(spec, &error))
    }
}

fn command(spec: &CommandSpec) -> Command {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args).envs(spec.env.iter().cloned()).kill_on_drop(true);
    if let Some(path) = running_folder().and_then(|folder| path_ending_in(&folder, spec)) {
        command.env("PATH", path);
    }
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    #[cfg(unix)]
    command.process_group(0);
    command
}

/// The folder of the running executable, links resolved.
fn running_folder() -> Option<PathBuf> {
    let exe = std::env::current_exe().and_then(std::fs::canonicalize).ok()?;
    exe.parent().map(Path::to_path_buf)
}

/// The `PATH` `spec` sets, else this process's, with `folder` added at its end; `None` when it
/// holds `folder` already, or is unset or empty, so the system's default search stays. An empty
/// entry means the working directory, so it is dropped rather than kept ahead of `folder`.
fn path_ending_in(folder: &Path, spec: &CommandSpec) -> Option<OsString> {
    let set = spec.env.iter().rev().find(|(name, _)| name == "PATH").map(|(_, value)| OsString::from(value));
    let path = set.or_else(|| std::env::var_os("PATH")).unwrap_or_default();
    let mut folders: Vec<PathBuf> =
        std::env::split_paths(&path).filter(|known| !known.as_os_str().is_empty()).collect();
    if folders.is_empty() || folders.iter().any(|known| known == folder) {
        return None;
    }
    folders.push(folder.to_path_buf());
    std::env::join_paths(folders).ok()
}

fn not_run(spec: &CommandSpec, error: &std::io::Error) -> CommandError {
    match error.kind() {
        ErrorKind::NotFound => CommandError::NotFound { program: spec.program.clone() },
        _ => CommandError::Failed { program: spec.program.clone(), reason: error.to_string() },
    }
}

/// The program's process group, killed when it is dropped while the program still runs: on a
/// timeout, or when the run drops the action.
struct Group {
    leader: Option<u32>,
    running: bool,
}

impl Drop for Group {
    fn drop(&mut self) {
        if let Some(leader) = self.leader.filter(|_| self.running) {
            kill_group(leader);
        }
    }
}

#[cfg(unix)]
fn kill_group(leader: u32) {
    use rustix::process::{Pid, Signal, kill_process_group};
    if let Some(pid) = i32::try_from(leader).ok().and_then(Pid::from_raw) {
        kill_process_group(pid, Signal::KILL).ok();
    }
}

/// Without process groups, `kill_on_drop` ends the program itself.
#[cfg(not(unix))]
const fn kill_group(_leader: u32) {}

/// Reads both streams while the program runs, and what they hold once it exits.
async fn collect(child: &mut Child, cap: usize) -> std::io::Result<CommandOutput> {
    let (mut stdout, mut stderr) = (child.stdout.take(), child.stderr.take());
    let (mut out, mut err) = (Tail::new(cap), Tail::new(cap));
    let status = {
        let reading = pin!(async { tokio::join!(out.read(stdout.as_mut()), err.read(stderr.as_mut())) });
        exited(child, reading).await
    };
    discard_rest(stdout.filter(|_| !out.ended), stderr.filter(|_| !err.ended));
    let ((stdout, out_cut), (stderr, err_cut)) = (out.finish(), err.finish());
    let mut output = CommandOutput::new(status?.code(), stdout);
    output.stderr = stderr;
    output.truncated = out_cut || err_cut;
    Ok(output)
}

/// The program's exit status, once it exits and `reading` ends or has had [`DRAIN`] more.
async fn exited(
    child: &mut Child,
    mut reading: Pin<&mut impl Future<Output = ((), ())>>,
) -> std::io::Result<ExitStatus> {
    tokio::select! {
        status = child.wait() => {
            tokio::time::timeout(DRAIN, reading).await.ok();
            status
        }
        ((), ()) = &mut reading => child.wait().await,
    }
}

/// Reads what the streams not yet ended still carry to their end in a detached task and drops
/// it: a process left in the background may write long after the run, and must not die of a
/// closed pipe while vc runs. With both ended, there is no task.
fn discard_rest(stdout: Option<ChildStdout>, stderr: Option<ChildStderr>) {
    async fn discard(stream: Option<impl AsyncRead + Unpin>) {
        if let Some(mut stream) = stream {
            tokio::io::copy(&mut stream, &mut tokio::io::sink()).await.ok();
        }
    }
    if stdout.is_some() || stderr.is_some() {
        drop(tokio::spawn(async move { tokio::join!(discard(stdout), discard(stderr)) }));
    }
}

/// The last `cap` bytes read from a stream, whether anything before them was dropped, and
/// whether the stream has ended.
struct Tail {
    kept: Vec<u8>,
    cut: bool,
    cap: usize,
    ended: bool,
}

impl Tail {
    const fn new(cap: usize) -> Self {
        Self { kept: Vec::new(), cut: false, cap, ended: false }
    }

    /// Reads `stream` to its end (or its first error), keeping its tail.
    async fn read(&mut self, stream: Option<&mut (impl AsyncRead + Unpin)>) {
        let Some(stream) = stream else { return };
        let mut chunk = vec![0; CHUNK];
        let room = self.cap.saturating_mul(2).max(CHUNK);
        while let Ok(read @ 1..) = stream.read(&mut chunk).await {
            self.kept.extend_from_slice(chunk.get(..read).unwrap_or_default());
            if self.kept.len() > room {
                self.cut |= trim(&mut self.kept, self.cap);
            }
        }
        self.ended = true;
    }

    /// The tail kept, and whether anything before it was dropped.
    fn finish(mut self) -> (Vec<u8>, bool) {
        let cut = trim(&mut self.kept, self.cap);
        (self.kept, self.cut || cut)
    }
}

/// Drops the front of `kept` past `cap` bytes; whether it dropped any.
fn trim(kept: &mut Vec<u8>, cap: usize) -> bool {
    let over = kept.len().saturating_sub(cap);
    kept.drain(..over);
    over > 0
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;
    use std::time::Duration;

    use vc_interfaces::{CommandError, CommandOutput, CommandRunner, CommandSpec};
    use vc_testkit::command_runner_suite;

    use super::{ProcessRunner, path_ending_in, trim};

    #[tokio::test]
    async fn the_process_runner_meets_the_command_runner_contract() {
        command_runner_suite(&ProcessRunner::new(), &CommandSpec::new("echo", ["hello"])).await;
    }

    #[tokio::test]
    async fn arguments_reach_the_program_as_they_are() {
        let spec = CommandSpec::new("echo", ["a b", "$HOME;", "`x`"]);
        let output = ProcessRunner::new().run(&spec).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "a b $HOME; `x`\n");
        let capped = ProcessRunner::new().run(&spec.with_output_cap(3)).await.unwrap();
        assert!(capped.truncated && capped.stdout == b"x`\n", "the tail is kept");
    }

    #[tokio::test]
    async fn a_slow_program_is_killed_at_its_timeout_and_a_failing_one_reports_its_status() {
        let slow = CommandSpec::new("sleep", ["5"]).with_timeout(Duration::from_millis(50));
        let error = ProcessRunner::new().run(&slow).await.unwrap_err();
        assert_eq!(error, CommandError::TimedOut { program: "sleep".into(), after: Duration::from_millis(50) });
        let failing = ProcessRunner::new().run(&CommandSpec::new("false", Vec::<String>::new()).in_dir("/")).await;
        assert_eq!(failing.unwrap().status, Some(1));
    }

    #[cfg(unix)]
    #[test]
    fn the_running_folder_goes_on_the_end_of_path_once() {
        let spec = CommandSpec::new("vc", Vec::<String>::new());
        let mut set = spec.clone();
        set.env.push(("PATH".into(), "/mine/bin:/usr/bin".into()));
        let ending = path_ending_in(Path::new("/opt/vc"), &set);
        assert_eq!(ending, Some(OsString::from("/mine/bin:/usr/bin:/opt/vc")), "after the user's own folders");
        assert_eq!(path_ending_in(Path::new("/mine/bin"), &set), None, "a folder already on PATH stays where it is");
        assert!(
            path_ending_in(Path::new("/opt/vc"), &spec)
                .is_some_and(|path| path.to_string_lossy().ends_with(":/opt/vc"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_empty_path_is_left_alone_and_empty_entries_are_dropped() {
        let with = |path: &str| {
            let mut spec = CommandSpec::new("vc", Vec::<String>::new());
            spec.env.push(("PATH".into(), path.into()));
            path_ending_in(Path::new("/opt/vc"), &spec)
        };
        assert_eq!(with(""), None, "an unset or empty PATH keeps the system's default search, not `:/opt/vc`");
        assert_eq!(with("::"), None);
        assert_eq!(with(":/usr/bin::"), Some(OsString::from("/usr/bin:/opt/vc")), "no working directory first");
    }

    #[test]
    fn only_the_last_bytes_are_kept() {
        let mut kept = b"abcdef".to_vec();
        assert!(trim(&mut kept, 4) && kept == b"cdef");
        assert!(!trim(&mut kept, 4) && kept == b"cdef");
        assert!(trim(&mut kept, 0) && kept.is_empty());
    }

    #[cfg(unix)]
    fn shell(line: &str, dir: &std::path::Path) -> CommandSpec {
        CommandSpec::new("sh", ["-c", line]).in_dir(dir)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_large_output_keeps_its_tail_per_stream() {
        let dir = tempfile::tempdir().unwrap();
        let line = "head -c 300000 /dev/zero | tr '\\0' a; echo end; echo err >&2";
        let output = ProcessRunner::new().run(&shell(line, dir.path()).with_output_cap(64 * 1024)).await.unwrap();
        assert_eq!(output.stdout.len(), 64 * 1024);
        assert!(output.stdout.ends_with(b"aaend\n") && output.stderr == b"err\n" && output.truncated);
        let whole = ProcessRunner::new().run(&shell("echo small", dir.path())).await.unwrap();
        assert!(!whole.truncated && whole.stdout == b"small\n");
    }

    /// Runs `line`, which leaves a process in the background and writes its pid to `child`;
    /// gives the run's output, how long it took and whether that process is alive `later`.
    #[cfg(unix)]
    async fn leaving_one_behind(line: &str, later: Duration) -> (CommandOutput, Duration, bool) {
        let dir = tempfile::tempdir().unwrap();
        let spec = shell(line, dir.path()).with_timeout(Duration::from_secs(10));
        let started = std::time::Instant::now();
        let output = ProcessRunner::new().run(&spec).await;
        let took = started.elapsed();
        tokio::time::sleep(later).await;
        let child = std::fs::read_to_string(dir.path().join("child")).unwrap();
        let alive = ProcessRunner::new().run(&shell(&format!("kill -0 {}", child.trim()), dir.path())).await;
        ProcessRunner::new().run(&shell(&format!("kill {}", child.trim()), dir.path())).await.unwrap();
        (output.unwrap(), took, alive.unwrap().status == Some(0))
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_run_ends_when_the_program_exits_though_a_background_process_holds_its_output() {
        let line = "sleep 30 & echo $! > child; echo started";
        let (output, took, alive) = leaving_one_behind(line, Duration::ZERO).await;
        assert_eq!((output.status, output.stdout), (Some(0), b"started\n".to_vec()));
        assert!(took < Duration::from_secs(5), "waited {took:?} for the background sleep");
        assert!(alive, "a clean exit leaves the background process running");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_background_process_that_writes_after_the_run_keeps_running() {
        let line = "(sleep 1; echo late; echo late >&2; exec sleep 30) & echo $! > child; echo started";
        let (output, took, alive) = leaving_one_behind(line, Duration::from_millis(1500)).await;
        assert_eq!((output.status, output.stdout), (Some(0), b"started\n".to_vec()));
        assert!(took < Duration::from_secs(1), "the run waited {took:?} for the background process");
        assert!(alive, "writing to the run's pipes after it ended killed the background process");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_program_whose_streams_ended_leaves_no_task_behind() {
        let dir = tempfile::tempdir().unwrap();
        let metrics = tokio::runtime::Handle::current().metrics();
        let output = ProcessRunner::new().run(&shell("echo done", dir.path())).await.unwrap();
        assert_eq!(output.stdout, b"done\n");
        assert_eq!(metrics.num_alive_tasks(), 0, "nothing is left to discard once both streams have ended");
        let line = "sleep 30 & echo $! > child; echo started";
        ProcessRunner::new().run(&shell(line, dir.path())).await.unwrap();
        assert_eq!(metrics.num_alive_tasks(), 1, "a stream a background process holds open is still discarded");
        let child = std::fs::read_to_string(dir.path().join("child")).unwrap();
        ProcessRunner::new().run(&shell(&format!("kill {}", child.trim()), dir.path())).await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_timeout_kills_the_whole_process_group() {
        let dir = tempfile::tempdir().unwrap();
        let line = "sleep 30 & echo $! > child; wait";
        let slow = shell(line, dir.path()).with_timeout(Duration::from_millis(300));
        assert!(matches!(ProcessRunner::new().run(&slow).await, Err(CommandError::TimedOut { .. })));
        let child = std::fs::read_to_string(dir.path().join("child")).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let probe = shell(&format!("kill -0 {}", child.trim()), dir.path());
        let alive = ProcessRunner::new().run(&probe).await.unwrap();
        assert_ne!(alive.status, Some(0), "the background sleep outlived the timeout");
    }
}
