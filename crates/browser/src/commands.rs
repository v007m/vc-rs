//! `command:` actions in the playground. The page has no processes, so this runner answers the
//! lines a page can honour and refuses the rest with a message: `vc demo start` and
//! `vc demo stop`, since the demo service already runs in the page, and a plain `echo`. A line
//! with shell syntax (`&&`, `|`, `;`, `$`, quotes, redirects, a newline) needs a shell, so it is
//! refused rather than half carried out.

use async_trait::async_trait;
use vc_interfaces::{CommandError, CommandOutput, CommandRunner, CommandSpec};

/// The CLI's names, as a `command:` line starts.
const VC: [&str; 2] = ["vc", "vibranium"];
/// The `vc demo` subcommands the page's demo service makes moot.
const DEMO: [&str; 2] = ["start", "stop"];
/// Why any other line does not run.
const NO_PROCESSES: &str = "the playground runs no programs: only `vc demo start`, `vc demo stop` and `echo` \
                            answer, since the demo service already runs in the page";
/// The characters that make a line need a shell.
const SHELL_SYNTAX: [char; 11] = ['&', '|', ';', '$', '`', '>', '<', '\'', '"', '\n', '\r'];
/// Why a line with shell syntax does not run.
const NEEDS_A_SHELL: &str = "this line needs a shell; run it with `vc run`";

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
        let program = || words.first().copied().unwrap_or_default().to_owned();
        if line.contains(SHELL_SYNTAX) {
            return Err(CommandError::Failed { program: program(), reason: NEEDS_A_SHELL.to_owned() });
        }
        match words.as_slice() {
            ["echo", rest @ ..] => Ok(CommandOutput::new(Some(0), format!("{}\n", rest.join(" ")))),
            [vc, "demo", sub, ..] if VC.contains(vc) && DEMO.contains(sub) => Ok(CommandOutput::new(Some(0), "")),
            _ => Err(CommandError::Failed { program: program(), reason: NO_PROCESSES.to_owned() }),
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

    #[tokio::test]
    async fn a_line_with_shell_syntax_needs_a_shell() {
        let lines = [
            "echo a && rm -rf x",
            "echo a || b",
            "echo a | cat",
            "echo a; b",
            "echo $HOME",
            "echo `id`",
            "echo a > out",
            "echo a < in",
            "echo 'a'",
            "echo \"a\"",
            "echo a\nb",
            "vc demo start && curl x",
        ];
        for text in lines {
            let refused = PageCommands.run(&line(text)).await.unwrap_err();
            let CommandError::Failed { program, reason } = &refused else { panic!("{text}: {refused:?}") };
            assert_eq!(reason, "this line needs a shell; run it with `vc run`", "{text}");
            assert_eq!(program, text.split_whitespace().next().unwrap(), "{text}");
        }
        let plain = PageCommands.run(&line("echo plain  words")).await.unwrap();
        assert_eq!(plain.stdout, b"plain words\n");
    }
}
