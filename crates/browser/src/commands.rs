//! `command:` actions in the playground. The page has no processes, so this runner answers the
//! lines a page can honour and refuses the rest with a message: `vc demo start` and
//! `vc demo stop`, since the demo service already runs in the page, and `echo`.

use async_trait::async_trait;
use vc_interfaces::{CommandError, CommandOutput, CommandRunner, CommandSpec};

/// The CLI's names, as a `command:` line starts.
const VC: [&str; 2] = ["vc", "vibranium"];
/// The `vc demo` subcommands the page's demo service makes moot.
const DEMO: [&str; 2] = ["start", "stop"];
/// Why any other line does not run.
const NO_PROCESSES: &str = "the playground runs no programs: only `vc demo start`, `vc demo stop` and `echo` \
                            answer, since the demo service already runs in the page";

/// Answers `command:` lines without a process.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct PageCommands;

#[async_trait]
impl CommandRunner for PageCommands {
    async fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, CommandError> {
        let line = match (spec.program.as_str(), spec.args.as_slice()) {
            ("sh", [flag, line]) if flag == "-c" => line.as_str(),
            (program, _) => return Err(CommandError::NotFound { program: program.to_owned() }),
        };
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["echo", rest @ ..] => Ok(CommandOutput::new(Some(0), format!("{}\n", rest.join(" ")))),
            [vc, "demo", sub, ..] if VC.contains(vc) && DEMO.contains(sub) => Ok(CommandOutput::new(Some(0), "")),
            _ => {
                let program = words.first().copied().unwrap_or_default().to_owned();
                Err(CommandError::Failed { program, reason: NO_PROCESSES.to_owned() })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::{CommandError, CommandRunner as _, CommandSpec};
    use vc_testkit::command_runner_suite;

    use super::PageCommands;

    fn line(text: &str) -> CommandSpec {
        CommandSpec::new("sh", ["-c", text])
    }

    #[tokio::test]
    async fn page_commands_meet_the_command_runner_contract() {
        command_runner_suite(&PageCommands, &line("echo hello")).await;
    }

    #[tokio::test]
    async fn the_demo_lines_succeed_and_others_say_why_not() {
        assert!(PageCommands.run(&line("vc demo start --port 7421 --detach")).await.unwrap().success());
        assert!(PageCommands.run(&line("vibranium demo stop")).await.unwrap().success());
        let refused = PageCommands.run(&line("curl https://example.com")).await.unwrap_err();
        let said = matches!(&refused, CommandError::Failed { program, reason } if program == "curl" && reason.contains("no programs"));
        assert!(said, "{refused:?}");
    }
}
