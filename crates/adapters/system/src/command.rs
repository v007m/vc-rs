//! Running programs (D163): arguments as a list, never through a shell.

use std::io::ErrorKind;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::process::Command;
use vc_interfaces::{CommandError, CommandOutput, CommandRunner, CommandSpec};

/// A [`CommandRunner`] that starts the program itself, with stdin closed, and kills it when
/// the timeout passes.
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
        let mut command = Command::new(&spec.program);
        command.args(&spec.args).envs(spec.env.iter().cloned()).stdin(Stdio::null()).kill_on_drop(true);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        let output = tokio::time::timeout(spec.timeout, command.output())
            .await
            .map_err(|_| CommandError::TimedOut { program: spec.program.clone(), after: spec.timeout })?;
        let output = output.map_err(|error| match error.kind() {
            ErrorKind::NotFound => CommandError::NotFound { program: spec.program.clone() },
            _ => CommandError::Failed { program: spec.program.clone(), reason: error.to_string() },
        })?;
        let mut collected = CommandOutput::new(output.status.code(), output.stdout);
        collected.stderr = output.stderr;
        Ok(collected.capped(spec.output_cap))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vc_interfaces::{CommandError, CommandRunner, CommandSpec};
    use vc_testkit::command_runner_suite;

    use super::ProcessRunner;

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
        assert!(capped.truncated && capped.stdout == b"a b");
    }

    #[tokio::test]
    async fn a_slow_program_is_killed_at_its_timeout_and_a_failing_one_reports_its_status() {
        let slow = CommandSpec::new("sleep", ["5"]).with_timeout(Duration::from_millis(50));
        let error = ProcessRunner::new().run(&slow).await.unwrap_err();
        assert_eq!(error, CommandError::TimedOut { program: "sleep".into(), after: Duration::from_millis(50) });
        let failing = ProcessRunner::new().run(&CommandSpec::new("false", Vec::<String>::new()).in_dir("/")).await;
        assert_eq!(failing.unwrap().status, Some(1));
    }
}
