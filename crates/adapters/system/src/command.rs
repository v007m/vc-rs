//! Running programs (D163, D181): arguments as a list, never through a shell of its own.

use std::io::ErrorKind;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt as _};
use tokio::process::{Child, Command};
use vc_interfaces::{CommandError, CommandOutput, CommandRunner, CommandSpec};

/// How much is read from a stream at a time.
const CHUNK: usize = 8 * 1024;

/// A [`CommandRunner`] that starts the program itself, with stdin closed.
///
/// It reads stdout and stderr as they come and keeps the last `output_cap` bytes of each, so a
/// chatty program never fills memory. On unix the program leads a process group of its own,
/// and a timeout, or dropping the run, kills the whole group; elsewhere it kills the program.
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
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    #[cfg(unix)]
    command.process_group(0);
    command
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

/// Reads both streams to their end, then waits for the exit.
async fn collect(child: &mut Child, cap: usize) -> std::io::Result<CommandOutput> {
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let ((stdout, out_cut), (stderr, err_cut), status) =
        tokio::join!(tail(stdout, cap), tail(stderr, cap), child.wait());
    let mut output = CommandOutput::new(status?.code(), stdout);
    output.stderr = stderr;
    output.truncated = out_cut || err_cut;
    Ok(output)
}

/// The last `cap` bytes of `stream`, and whether anything before them was dropped.
async fn tail(stream: Option<impl AsyncRead + Unpin>, cap: usize) -> (Vec<u8>, bool) {
    let Some(mut stream) = stream else { return (Vec::new(), false) };
    let (mut kept, mut cut) = (Vec::new(), false);
    let mut chunk = vec![0; CHUNK];
    let room = cap.saturating_mul(2).max(CHUNK);
    while let Ok(read @ 1..) = stream.read(&mut chunk).await {
        kept.extend_from_slice(chunk.get(..read).unwrap_or_default());
        if kept.len() > room {
            cut |= trim(&mut kept, cap);
        }
    }
    cut |= trim(&mut kept, cap);
    (kept, cut)
}

/// Drops the front of `kept` past `cap` bytes; whether it dropped any.
fn trim(kept: &mut Vec<u8>, cap: usize) -> bool {
    let over = kept.len().saturating_sub(cap);
    kept.drain(..over);
    over > 0
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vc_interfaces::{CommandError, CommandRunner, CommandSpec};
    use vc_testkit::command_runner_suite;

    use super::{ProcessRunner, trim};

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
