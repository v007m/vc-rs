//! A schedule exported to CI (D520, D707): a GitHub Actions workflow that runs it on its cron,
//! for machines that sleep. The engine fixes the path, `.github/workflows/vibranium-<id>.yml`;
//! an existing workflow is the user's to edit, so the export is refused rather than written over.

use vc_model::{Schedule, SourcePath};
use vc_parser::write_text;

use crate::authoring::refused;
use crate::edit::{Changes, FileChange, internal};
use crate::explain::did_you_mean;
use crate::project::sources_error;
use crate::{Engine, EngineError};

/// The workflow [`Engine::schedule_export`] would write, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScheduleExport {
    pub changes: Changes,
    /// Relative to the project root.
    pub path: String,
}

const FOLDER: &str = ".github/workflows";

impl Engine {
    /// The new workflow file that runs the schedule `id` on GitHub Actions.
    ///
    /// # Errors
    ///
    /// The errors of compiling the project; [`EngineError::Edit`] when there is no such
    /// schedule or its workflow is there already.
    pub fn schedule_export(&self, id: &str) -> Result<ScheduleExport, EngineError> {
        let (program, _) = self.program()?;
        let schedules = &program.config.schedules;
        let Some(schedule) = schedules.iter().find(|schedule| schedule.id == id) else {
            let near = vc_planner::closest(id, schedules.iter().map(|schedule| schedule.id.as_str()));
            let help = did_you_mean(&near).unwrap_or_else(|| "the Runs space lists the schedules".to_owned());
            return Err(refused("there is no such schedule in the config", &help));
        };
        if !vc_compiler::is_camel_id(&schedule.id) {
            return Err(refused("a schedule's `id` is a camelCase word", "rename it, such as `nightlySmoke`"));
        }
        let path = format!("{FOLDER}/vibranium-{}.yml", schedule.id);
        let source = SourcePath::new(&path).map_err(internal)?;
        if !self.sources.read(&[source]).map_err(sources_error)?.files.is_empty() {
            let help = "edit that workflow by hand, or move it away to export the schedule again";
            return Err(refused(format!("`{path}` is there already"), help));
        }
        let change = FileChange { path: path.clone(), before: None, after: workflow(schedule), moved_from: None };
        let title = format!("Export schedule {} to a GitHub Actions workflow", schedule.id);
        Ok(ScheduleExport { changes: Changes { files: vec![change], titles: vec![title] }, path })
    }
}

/// The workflow that runs `schedule`.
fn workflow(schedule: &Schedule) -> String {
    let id = &schedule.id;
    let named = schedule.name.as_deref().map_or_else(String::new, |name| format!(" ({})", one_line(name)));
    let install = format!(
        "cargo install --git https://github.com/v007m/vc-rs --tag v{} vc-cli --locked",
        env!("CARGO_PKG_VERSION")
    );
    [
        format!("# The Vibranium schedule `{id}`{named}, exported from vibranium.config.yaml."),
        "# GitHub runs it on the cron below, in UTC. Set the schedule's `runner: githubActions` so".to_owned(),
        "# `vc ui` leaves it to this workflow, and add under `env:` the secrets its profiles read.".to_owned(),
        "# The schedule's `notify` is sent by `vc ui` only: here, the config's `alerts` apply.".to_owned(),
        format!("name: {}", write_text(&format!("Vibranium schedule {id}"), false)),
        "on:".to_owned(),
        "  schedule:".to_owned(),
        format!("    - cron: {}", write_text(schedule.cron.text(), false)),
        "  workflow_dispatch:".to_owned(),
        "permissions:".to_owned(),
        "  contents: read".to_owned(),
        "jobs:".to_owned(),
        "  run:".to_owned(),
        "    runs-on: ubuntu-latest".to_owned(),
        "    steps:".to_owned(),
        "      - uses: actions/checkout@v4".to_owned(),
        "      - name: Install vc".to_owned(),
        format!("        run: {install}"),
        "      - name: Run the schedule".to_owned(),
        format!("        run: {}", write_text(&command(schedule), false)),
        String::new(),
    ]
    .join("\n")
}

/// The `vc run` command line, each word quoted for the shell where it needs it.
fn command(schedule: &Schedule) -> String {
    let mut words = vec!["vc".to_owned(), "run".to_owned()];
    words.extend(schedule.selector.iter().map(|word| shell(word)));
    for profile in &schedule.profiles {
        words.extend(["--profile".to_owned(), shell(profile)]);
    }
    words.join(" ")
}

/// `word` as one shell word: as it is when it holds only plain characters, else single-quoted.
fn shell(word: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "._-/:@".contains(c);
    if !word.is_empty() && word.chars().all(plain) {
        return word.to_owned();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::shell;

    #[test]
    fn a_word_with_a_shell_character_is_quoted() {
        assert_eq!(shell("users.crud"), "users.crud");
        assert_eq!(shell("#listsUsers"), "'#listsUsers'", "a `#` would start a comment");
        assert_eq!(shell("it's"), r"'it'\''s'");
        assert_eq!(shell(""), "''");
    }
}
