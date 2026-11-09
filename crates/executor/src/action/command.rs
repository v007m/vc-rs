//! `command:` actions: a shell line run through the [`CommandRunner`] port (D181).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use vc_diagnostics::Code;
use vc_interfaces::{CommandError, CommandOutput, CommandRunner, CommandSpec};
use vc_model::{Action, ActionId, ActionKind, Failure, Namespace, Part, Path, Program, Reference, Segment, Template};

use super::request;
use super::{ActionExecutor, ActionRequest};
use crate::{ActionReport, Render};

/// How long a command may run when the config's `defaults` sets no `timeout`.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// How much of each stream a `command:` keeps: its last 64 KiB (D181).
const KEPT: usize = 64 * 1024;

/// How many lines of output a failure shows.
const SHOWN_LINES: usize = 20;

/// The prefix the run's vars are exported under: `VIB_VAR_userId` (D182).
const VAR_PREFIX: &str = "VIB_VAR_";

/// What `cmd` acts on inside a double-quoted word; a value holding one is refused.
const CMD_SPECIAL: &[char] = &['&', '|', '<', '>', '^', '%', '!', '"', '(', ')', '\n', '\r'];

/// Turns one rendered value into one shell word.
type Word = fn(&str) -> Result<String, Failure>;

/// The shell, its flag and how it quotes a value: `sh -c` on unix, `cmd /C` on Windows.
const SHELL: (&str, &str, Word) = if cfg!(windows) { ("cmd", "/C", cmd_word) } else { ("sh", "-c", posix_word) };

/// Runs `command:` actions through a [`CommandRunner`].
///
/// The line runs in the project root through the system shell. Literal text goes to the shell
/// as written; each `${…}` is rendered and quoted as one word. The child gets `env` (the
/// `.env` values the process does not set) and the run's config vars as `VIB_VAR_<name>`, over
/// the process environment. A non-zero exit fails the action (V5110) with the exit code and the
/// last lines of output; the limit is the config's `defaults.timeout` (V5104).
#[derive(Clone)]
pub struct CommandExecutor {
    runner: Arc<dyn CommandRunner>,
    root: PathBuf,
    env: Vec<(String, String)>,
}

impl std::fmt::Debug for CommandExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandExecutor").field("root", &self.root).finish_non_exhaustive()
    }
}

impl CommandExecutor {
    /// An executor that runs each line with `runner` in `root`, adding `env` to the child's.
    #[must_use]
    pub fn new(runner: Arc<dyn CommandRunner>, root: PathBuf, env: Vec<(String, String)>) -> Self {
        Self { runner, root, env }
    }

    async fn command(&self, request: &ActionRequest) -> Result<ActionReport, Failure> {
        let action =
            request.cx.action(request.action).ok_or_else(|| Failure::new("the action is not in the program"))?;
        let ActionKind::Command(template) = &action.kind else {
            return Err(Failure::new("the action runs no command"));
        };
        let render = Render::new(request).map_err(request::failed)?;
        let spec = self.spec(&render, template, action)?;
        let output = self.runner.run(&spec).await.map_err(|error| not_run(&error))?;
        if !output.success() {
            return Err(exited(&output));
        }
        let warnings = render.warnings();
        Ok(ActionReport { fakes: render.into_memo(), warnings, ..ActionReport::passed() })
    }

    fn spec(&self, render: &Render<'_>, template: &Template, action: &Action) -> Result<CommandSpec, Failure> {
        let (shell, flag, _) = SHELL;
        let timeout = action.timeout.or_else(|| render.program().config.defaults.timeout);
        let timeout = timeout.map_or(DEFAULT_TIMEOUT, vc_model::Duration::get);
        let mut spec = CommandSpec::new(shell, [flag.to_owned(), line(render, template)?])
            .in_dir(self.root.clone())
            .with_timeout(timeout)
            .with_output_cap(KEPT);
        spec.env.extend(self.env.iter().cloned());
        spec.env.extend(exported(render, action));
        Ok(spec)
    }
}

#[async_trait]
impl ActionExecutor for CommandExecutor {
    async fn execute(&self, request: ActionRequest) -> ActionReport {
        self.command(&request).await.unwrap_or_else(|failure| ActionReport::failed(failure.at(request.action)))
    }

    fn follows(&self, _program: &Program, _action: &Action) -> Vec<ActionId> {
        vec![]
    }
}

/// The line: literals as written, each reference rendered and quoted as one word.
fn line(render: &Render<'_>, template: &Template) -> Result<String, Failure> {
    let (_, _, word) = SHELL;
    let part = |part: &Part| match part {
        Part::Literal(text) => Ok(text.clone()),
        Part::Ref(reference) => word(&render.text(&lone(reference.clone())).map_err(request::failed)?),
    };
    template.parts.iter().map(part).collect()
}

/// The run's config vars that render in the action's scope, as `VIB_VAR_<name>`; a var that
/// does not render here, such as one reading a response, is left out.
fn exported(render: &Render<'_>, action: &Action) -> Vec<(String, String)> {
    let mut names: Vec<String> = render.values().config().keys().cloned().collect();
    names.sort_unstable();
    let var = |name: String| {
        let reference = Reference {
            ns: Namespace::Var,
            path: Path { segments: vec![Segment::Key(name.clone())] },
            args: Vec::new(),
            pipes: Vec::new(),
            span: action.span,
            root_span: action.span,
        };
        Some((format!("{VAR_PREFIX}{name}"), render.text(&lone(reference)).ok()?))
    };
    names.into_iter().filter_map(var).collect()
}

fn lone(reference: Reference) -> Template {
    Template { parts: vec![Part::Ref(reference)] }
}

/// One POSIX shell word: single quotes, with `'` written as `'\''`.
#[expect(clippy::unnecessary_wraps, reason = "shares the `Word` signature with `cmd_word`, which can fail")]
fn posix_word(value: &str) -> Result<String, Failure> {
    Ok(format!("'{}'", value.replace('\'', r"'\''")))
}

/// One `cmd` word: double quotes, refusing a value `cmd` would still act on (V5111).
fn cmd_word(value: &str) -> Result<String, Failure> {
    value.chars().find(|found| CMD_SPECIAL.contains(found)).map_or_else(
        || Ok(format!("\"{value}\"")),
        |found| {
            Err(request::rendering(&format!(
                "a value in the command holds {found:?}, which `cmd` would act on; pass it through a var instead"
            )))
        },
    )
}

/// The failure for a command that could not be run, or ran past its timeout (V5104).
fn not_run(error: &CommandError) -> Failure {
    let code = if matches!(error, CommandError::TimedOut { .. }) { Code::V5104 } else { Code::V5110 };
    Failure::new(error.to_string()).with_code(code.to_string())
}

/// The V5110 failure for a command that did not exit with 0.
fn exited(output: &CommandOutput) -> Failure {
    let ended = output.status.map_or_else(|| "was ended by a signal".to_owned(), |code| format!("exited with {code}"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let shown = if stderr.trim().is_empty() { String::from_utf8_lossy(&output.stdout) } else { stderr };
    let tail = last_lines(&shown, SHOWN_LINES);
    let message =
        if tail.is_empty() { format!("the command {ended}") } else { format!("the command {ended}:\n{tail}") };
    Failure::new(message).with_code(Code::V5110.to_string())
}

/// The last `count` lines of `text`, without the final line break.
fn last_lines(text: &str, count: usize) -> &str {
    let text = text.trim_end_matches(['\n', '\r']);
    text.rmatch_indices('\n').nth(count.saturating_sub(1)).map_or(text, |(at, _)| text.get(at + 1..).unwrap_or(text))
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
