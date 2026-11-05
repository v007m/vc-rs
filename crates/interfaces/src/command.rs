//! Running a program, such as `varlock load` (D163) or a `command` action.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

/// The longest a command runs when the spec does not say.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// The most a runner keeps of each output stream when the spec does not say: 4 MiB.
const DEFAULT_OUTPUT_CAP: usize = 4 << 20;

/// One program to run, with its arguments as a list: never through a shell.
///
/// `Debug` shows the program and the argument count, and the env names without their values.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandSpec {
    /// The program, found on `PATH` when it has no `/`.
    pub program: String,
    /// The arguments, each passed as it is.
    pub args: Vec<String>,
    /// The working directory; the runner's own when `None`.
    pub cwd: Option<PathBuf>,
    /// Variables added to the inherited environment.
    pub env: Vec<(String, String)>,
    /// How long it may run before it is killed.
    pub timeout: Duration,
    /// The most bytes kept of stdout and of stderr; the rest is dropped and the output is
    /// marked [`CommandOutput::truncated`].
    pub output_cap: usize,
}

impl CommandSpec {
    /// `program` with `args`, a 30s timeout, a 4 MiB output cap and nothing else.
    #[must_use]
    pub fn new(program: impl Into<String>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            cwd: None,
            env: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
            output_cap: DEFAULT_OUTPUT_CAP,
        }
    }

    /// This spec, run in `cwd`.
    #[must_use]
    pub fn in_dir(self, cwd: impl Into<PathBuf>) -> Self {
        Self { cwd: Some(cwd.into()), ..self }
    }

    /// This spec, killed after `timeout`.
    #[must_use]
    pub fn with_timeout(self, timeout: Duration) -> Self {
        Self { timeout, ..self }
    }

    /// This spec, keeping at most `cap` bytes of each output stream.
    #[must_use]
    pub fn with_output_cap(self, cap: usize) -> Self {
        Self { output_cap: cap, ..self }
    }
}

impl fmt::Debug for CommandSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env: Vec<&str> = self.env.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("CommandSpec")
            .field("program", &self.program)
            .field("args", &self.args.len())
            .field("cwd", &self.cwd)
            .field("env", &env)
            .field("timeout", &self.timeout)
            .field("output_cap", &self.output_cap)
            .finish()
    }
}

/// What a finished program left. `Debug` shows the sizes, never the output, which may hold
/// secrets.
#[derive(Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandOutput {
    /// The exit code; `None` when a signal ended it.
    pub status: Option<i32>,
    /// Everything it wrote to stdout.
    pub stdout: Vec<u8>,
    /// Everything it wrote to stderr.
    pub stderr: Vec<u8>,
    /// Whether either stream went past the spec's `output_cap` and was cut there.
    pub truncated: bool,
}

impl CommandOutput {
    /// An output with `status` and `stdout`, and no stderr.
    #[must_use]
    pub fn new(status: Option<i32>, stdout: impl Into<Vec<u8>>) -> Self {
        Self { status, stdout: stdout.into(), stderr: Vec::new(), truncated: false }
    }

    /// This output with each stream cut to its last `cap` bytes, as the runner keeps them,
    /// marked truncated when either was longer.
    #[must_use]
    pub fn capped(mut self, cap: usize) -> Self {
        self.truncated |= self.stdout.len() > cap || self.stderr.len() > cap;
        tail(&mut self.stdout, cap);
        tail(&mut self.stderr, cap);
        self
    }

    /// Whether it exited with code 0.
    #[must_use]
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
}

/// Drops all but the last `cap` bytes of `stream`.
fn tail(stream: &mut Vec<u8>, cap: usize) {
    stream.drain(..stream.len().saturating_sub(cap));
}

impl fmt::Debug for CommandOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandOutput")
            .field("status", &self.status)
            .field("stdout_len", &self.stdout.len())
            .field("stderr_len", &self.stderr.len())
            .field("truncated", &self.truncated)
            .finish()
    }
}

/// Runs one program to completion and collects its output.
#[async_trait]
pub trait CommandRunner: Send + Sync {
    /// Runs `spec`. A non-zero exit is an [`Ok`] output; only failing to run is an error.
    ///
    /// # Errors
    ///
    /// [`CommandError`] when the program cannot start or outlives its timeout.
    async fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, CommandError>;
}

/// Why a program could not be run to completion.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CommandError {
    /// The program is not on `PATH` or not executable.
    #[error("cannot find `{program}`")]
    NotFound {
        /// The program as named.
        program: String,
    },
    /// The program could not be started for another reason.
    #[error("cannot run `{program}`: {reason}")]
    Failed {
        /// The program as named.
        program: String,
        /// What went wrong.
        reason: String,
    },
    /// The program ran past its timeout and was killed.
    #[error("`{program}` did not finish within {}ms", after.as_millis())]
    TimedOut {
        /// The program as named.
        program: String,
        /// The timeout.
        after: Duration,
    },
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{CommandError, CommandOutput, CommandSpec};

    #[test]
    fn debug_hides_env_values_and_output() {
        let mut spec = CommandSpec::new("varlock", ["load", "--format", "json"]).in_dir("/p");
        spec.env.push(("TOKEN".into(), "s3cr3t".into()));
        let shown = format!("{spec:?}");
        assert!(shown.contains("TOKEN") && !shown.contains("s3cr3t") && shown.contains("args: 3"), "{shown}");
        let output = CommandOutput::new(Some(0), b"{\"A\":\"s3cr3t\"}".to_vec());
        assert!(!format!("{output:?}").contains("s3cr3t"));
        assert!(output.success() && !CommandOutput::new(Some(1), []).success() && !CommandOutput::default().success());
        assert!(!output.clone().capped(14).truncated && output.clone().capped(14).stdout.len() == 14);
        let mut noisy = output.capped(4);
        assert!(noisy.truncated && noisy.stdout == b"3t\"}");
        noisy.stderr = b"12345".to_vec();
        noisy.truncated = false;
        let noisy = noisy.capped(4);
        assert!(noisy.truncated && noisy.stderr == b"2345" && noisy.stdout == b"3t\"}");
    }

    #[test]
    fn a_spec_defaults_to_30s_and_messages_name_the_program() {
        let spec = CommandSpec::new("x", Vec::<String>::new());
        assert_eq!((spec.timeout, spec.cwd.clone()), (Duration::from_secs(30), None));
        assert_eq!(spec.output_cap, 4 * 1024 * 1024);
        assert_eq!(spec.clone().with_output_cap(64).output_cap, 64);
        assert_eq!(spec.with_timeout(Duration::from_secs(2)).timeout, Duration::from_secs(2));
        let timed_out = CommandError::TimedOut { program: "x".into(), after: Duration::from_millis(5) };
        assert_eq!(timed_out.to_string(), "`x` did not finish within 5ms");
        assert_eq!(CommandError::NotFound { program: "x".into() }.to_string(), "cannot find `x`");
    }
}
