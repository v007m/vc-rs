use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use vc_diagnostics::Code;
use vc_interfaces::{CommandError, CommandOutput, CommandRunner, CommandSpec};
use vc_model::{ActionId, ActionKind, ActionOutcome, Failure, OwnerKey};
use vc_testkit::FakeCommands;

use super::{CommandExecutor, SHELL, cmd_word, last_lines, posix_word};
use crate::fixture::Fixture;
use crate::{ActionExecutors, ActionKindTag, ActionReport, ActionRequest, Frame};

/// A runner that sends each spec it is given down a channel and answers each the same way.
fn runner(answer: Result<CommandOutput, CommandError>) -> (Arc<dyn CommandRunner>, Receiver<CommandSpec>) {
    let (ran, specs) = mpsc::channel();
    let fake = FakeCommands::new(move |spec| {
        ran.send(spec.clone()).ok();
        Some(answer.clone())
    });
    (Arc::new(fake), specs)
}

fn exiting(status: i32, stderr: &str) -> CommandOutput {
    let mut output = CommandOutput::new(Some(status), "out\n");
    output.stderr = stderr.as_bytes().to_vec();
    output
}

async fn run(mut fixture: Fixture, line: &str, runner: Arc<dyn CommandRunner>) -> ActionReport {
    let template = fixture.template(line);
    let action = fixture.action(ActionKind::Command(template));
    let dot_env = vec![("FROM_DOTENV".to_owned(), "yes".to_owned())];
    let executor = CommandExecutor::new(runner, PathBuf::from("/project"), dot_env);
    let executors = ActionExecutors::new().with(ActionKindTag::Command, Arc::new(executor));
    let cx = fixture.cx(executors);
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    cx.executors.execute(request).await
}

fn failure(report: &ActionReport) -> Option<(&str, &str)> {
    report.outcome.failure().map(|failure: &Failure| (failure.code.as_deref().unwrap_or(""), failure.message.as_str()))
}

#[tokio::test]
async fn runs_the_line_in_the_root_with_values_quoted_and_vars_exported() {
    let mut fixture = Fixture::new();
    fixture.program.config.vars.values = fixture.vars(&[("userId", "it's; exit 7"), ("token", "${env.VC_NOT_SET_9Q}")]);
    let (commands, specs) = runner(Ok(exiting(0, "")));
    let report = run(fixture, "seed --user ${var.userId} && echo done", commands).await;
    assert_eq!(report.outcome, ActionOutcome::Passed);
    let spec = specs.try_recv().unwrap();
    let (shell, flag, word) = SHELL;
    let line = format!("seed --user {} && echo done", word("it's; exit 7").unwrap_or_default());
    assert_eq!((spec.program.as_str(), spec.args.clone()), (shell, vec![flag.to_owned(), line]));
    assert_eq!(spec.cwd, Some(PathBuf::from("/project")));
    let env = [("FROM_DOTENV", "yes"), ("VIB_VAR_userId", "it's; exit 7")].map(|(k, v)| (k.to_owned(), v.to_owned()));
    assert_eq!(spec.env, env, "a var that does not render here is left out");
    assert_eq!((spec.timeout, spec.output_cap), (Duration::from_secs(30), 64 * 1024));
}

#[tokio::test]
async fn the_config_timeout_limits_the_command() {
    let mut fixture = Fixture::new();
    fixture.program.config.defaults.timeout = Some(vc_model::Duration::new(Duration::from_millis(250)));
    let (commands, specs) = runner(Ok(exiting(0, "")));
    run(fixture, "true", commands).await;
    assert_eq!(specs.try_recv().unwrap().timeout, Duration::from_millis(250));
}

#[tokio::test]
async fn a_non_zero_exit_fails_with_the_code_and_the_last_lines() {
    let stderr = (1..=25).map(|n| format!("line {n}\n")).collect::<Vec<_>>().concat();
    let (commands, _) = runner(Ok(exiting(3, &stderr)));
    let report = run(Fixture::new(), "migrate", commands).await;
    let (code, message) = failure(&report).unwrap();
    assert_eq!(code, Code::V5110.to_string());
    assert!(message.starts_with("the command exited with 3:\nline 6\n"), "{message}");
    assert!(message.ends_with("line 25"), "{message}");
    assert_eq!(report.outcome.failure().and_then(|failure| failure.action), Some(ActionId::new(0)));
}

#[tokio::test]
async fn stdout_is_shown_when_stderr_is_empty_and_a_signal_is_named() {
    let (commands, _) = runner(Ok(CommandOutput::new(None, "partial\n")));
    let report = run(Fixture::new(), "work", commands).await;
    assert_eq!(failure(&report).map(|(_, message)| message), Some("the command was ended by a signal:\npartial"));
    let (commands, _) = runner(Ok(exiting(1, " \n")));
    let report = run(Fixture::new(), "work", commands).await;
    assert_eq!(failure(&report).map(|(_, message)| message), Some("the command exited with 1:\nout"));
}

#[tokio::test]
async fn a_timeout_or_a_missing_shell_fails_with_its_code() {
    let after = Duration::from_millis(50);
    let (commands, _) = runner(Err(CommandError::TimedOut { program: "sh".into(), after }));
    let report = run(Fixture::new(), "sleep 5", commands).await;
    assert_eq!(failure(&report), Some((Code::V5104.to_string().as_str(), "`sh` did not finish within 50ms")));
    let (commands, _) = runner(Err(CommandError::NotFound { program: "sh".into() }));
    let report = run(Fixture::new(), "x", commands).await;
    assert_eq!(failure(&report).map(|(code, _)| code), Some(Code::V5110.to_string().as_str()));
}

#[tokio::test]
async fn a_value_that_does_not_render_runs_nothing() {
    let (commands, specs) = runner(Ok(exiting(0, "")));
    let report = run(Fixture::new(), "seed ${var.userId}", commands).await;
    assert_eq!(failure(&report), Some((Code::V5111.to_string().as_str(), "`var.userId` is not set")));
    assert!(specs.try_recv().is_err());
}

#[tokio::test]
async fn a_fake_var_the_line_draws_is_kept_for_the_instance() {
    let mut fixture = Fixture::new();
    fixture.program.config.vars.values = fixture.vars(&[("userId", "${fake.string.uuid}")]);
    let (commands, specs) = runner(Ok(exiting(0, "")));
    let report = run(fixture, "seed ${var.userId}", commands).await;
    let drawn = report.fakes.get("userId").and_then(|id| id.as_str()).unwrap_or_default().to_owned();
    assert_eq!(drawn.len(), 36, "{:?}", report.fakes);
    assert!(specs.try_recv().unwrap().args[1].contains(&drawn));
}

#[test]
fn debug_shows_the_root_and_not_the_env() {
    let (commands, _) = runner(Ok(exiting(0, "")));
    let executor = CommandExecutor::new(commands, PathBuf::from("/project"), vec![("TOKEN".into(), "s3cret".into())]);
    assert_eq!(format!("{executor:?}"), "CommandExecutor { root: \"/project\", .. }");
}

#[test]
fn a_posix_word_is_single_quoted_with_quotes_escaped() {
    assert_eq!(posix_word("a b; exit 7").unwrap(), "'a b; exit 7'");
    assert_eq!(posix_word("it's").unwrap(), r"'it'\''s'");
    assert_eq!(posix_word("$(rm -rf /)").unwrap(), "'$(rm -rf /)'");
    assert_eq!(posix_word("").unwrap(), "''");
}

#[test]
fn a_cmd_word_refuses_what_cmd_acts_on() {
    assert_eq!(cmd_word("a b;c").unwrap(), "\"a b;c\"");
    for bad in ["a&b", "a|b", "a<b", "a>b", "a^b", "%PATH%", "a!b", "a\"b", "(a)", "a\nb", "a\rb"] {
        let error = cmd_word(bad).unwrap_err();
        assert_eq!(error.code.as_deref(), Some(Code::V5111.to_string().as_str()), "{bad}");
    }
}

#[test]
fn the_last_lines_drop_the_final_break() {
    assert_eq!(last_lines("a\nb\nc\n", 2), "b\nc");
    assert_eq!(last_lines("a\nb\n", 5), "a\nb");
    assert_eq!(last_lines("only", 1), "only");
    assert_eq!(last_lines("", 3), "");
}
