//! The config's `schedules` for the Runs space (D520, D705–D706): each listed with when it next
//! fires and its last run, one added, replaced or removed through the compile guard, and the
//! ones due in a minute for `vc ui`'s scheduler.

use vc_interfaces::RunFilter;
use vc_model::{Cron, ScheduleRunner};
use vc_parser::{Node, write_text};

use crate::authoring::{Opened, item, refused};
use crate::create::{DOCS, ListItem, block, commented_item};
use crate::edit::{Changes, FileChange, internal};
use crate::explain::did_you_mean;
use crate::plugins::config;
use crate::{Engine, EngineError, RunRow, SelectRequest};

/// One schedule as the config writes it: what the Runs space shows and sends back to change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps from what a client sends; every user is in this repo (D160)"
)]
pub struct ScheduleSpec {
    /// camelCase, unique among the schedules.
    pub id: String,
    pub name: Option<String>,
    /// Five cron fields in UTC, as written.
    pub cron: String,
    /// What runs, as `vc run`'s selectors; every test when empty.
    pub selector: Vec<String>,
    pub profiles: Vec<String>,
    /// The channels told when a scheduled run fails or is blocked.
    pub notify: Vec<NotifyChannel>,
    pub runner: ScheduleRunner,
    pub enabled: bool,
}

/// One `notify` entry: its kind and its settings as written, `${secret.…}` references and all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps from what a client sends; every user is in this repo (D160)"
)]
pub struct NotifyChannel {
    pub kind: String,
    pub settings: Vec<(String, String)>,
}

/// A schedule with when it next fires and its newest run.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScheduleListing {
    pub spec: ScheduleSpec,
    /// When `vc ui`'s scheduler next starts it; `None` when it is disabled, left to GitHub
    /// Actions, or its cron does not parse or matches no day.
    pub next_ms: Option<u64>,
    pub last: Option<RunRow>,
}

/// What [`Engine::schedule_write`] changes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduleEdit {
    /// Add the schedule, or put it in place of the one with its `id`.
    Put(ScheduleSpec),
    /// Remove the schedule with this `id`.
    Remove(String),
}

/// A schedule that is due: what the scheduler runs.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DueSchedule {
    pub id: String,
    /// Its selector words: a word with a `/` is a folder or test file, any other an ID, a file
    /// ID or a glob over IDs (D708).
    pub select: SelectRequest,
    pub profiles: Vec<String>,
}

const LIST: &str = "schedules";

impl Engine {
    /// The config's schedules in written order, each with its next firing after now and its
    /// newest run in the history. It reads the config as written, so a schedule whose cron is
    /// wrong is still listed.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when the config does not parse; [`EngineError::History`] when the
    /// history cannot be read.
    pub fn schedules(&self) -> Result<Vec<ScheduleListing>, EngineError> {
        let project = self.authoring()?;
        let opened = config(&project)?;
        let now = self.clock.now_ms();
        let specs = opened.document.root().get(LIST).map(Node::items).unwrap_or_default().iter().filter_map(spec);
        specs.map(|spec| self.listing(spec, now)).collect()
    }

    fn listing(&self, spec: ScheduleSpec, now: u64) -> Result<ScheduleListing, EngineError> {
        let fires = spec.enabled && spec.runner == ScheduleRunner::VcServer;
        let next_ms = fires.then(|| Cron::parse(&spec.cron).ok()?.next_after(now)).flatten();
        let filter = RunFilter { schedule: Some(spec.id.clone()), ..RunFilter::default() };
        let last = self.history(1, &filter)?.runs().first().cloned();
        Ok(ScheduleListing { spec, next_ms, last })
    }

    /// The enabled schedules `vc ui`'s server runs whose cron fires in the minute that starts at
    /// `minute_ms`, from the compiled program, so a project with errors runs none.
    ///
    /// # Errors
    ///
    /// The errors of compiling the project.
    pub fn due_schedules(&self, minute_ms: u64) -> Result<Vec<DueSchedule>, EngineError> {
        let (program, _) = self.program()?;
        let due = program.config.schedules.iter().filter(|schedule| {
            schedule.enabled && schedule.runner == ScheduleRunner::VcServer && schedule.cron.fires_at(minute_ms)
        });
        let due = due.map(|schedule| DueSchedule {
            id: schedule.id.clone(),
            select: selected(&schedule.selector),
            profiles: schedule.profiles.clone(),
        });
        Ok(due.collect())
    }

    /// The config change that adds, replaces or removes one schedule, checked by the compile
    /// guard, so a cron that does not parse or a profile the config lacks is refused.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when the `id`, a channel kind or a setting key is not a plain
    /// word, the schedule to remove is not there, or the change would add an error.
    pub fn schedule_write(&self, edit: &ScheduleEdit) -> Result<Changes, EngineError> {
        let project = self.authoring()?;
        let opened = config(&project)?;
        let changes = match edit {
            ScheduleEdit::Put(spec) => put(&opened, spec)?,
            ScheduleEdit::Remove(id) => remove(&opened, id)?,
        };
        self.guarded(&project, changes)
    }
}

/// The selection `words` make: each with a `/` a project path, each other an ID.
fn selected(words: &[String]) -> SelectRequest {
    let (paths, ids) = words.iter().cloned().partition(|word: &String| word.contains('/'));
    SelectRequest { ids, paths, ..SelectRequest::default() }
}

fn put(opened: &Opened<'_>, spec: &ScheduleSpec) -> Result<Changes, EngineError> {
    let yaml = yaml(spec)?;
    let comment = format!("# {} · schedule · keys: {DOCS}#schedules", spec.id);
    let (change, title) = match item(opened.document.root(), LIST, &spec.id) {
        Some(node) => (replaced(opened, node, (&yaml, &comment))?, "Change"),
        None => (commented_item(opened, &ListItem { list: LIST, yaml: &yaml, comment: &comment })?.0, "Add"),
    };
    Ok(Changes { files: vec![change], titles: vec![format!("{title} schedule {}", spec.id)] })
}

fn remove(opened: &Opened<'_>, id: &str) -> Result<Changes, EngineError> {
    let root = opened.document.root();
    let Some(node) = item(root, LIST, id) else {
        let known: Vec<String> = root.get(LIST).map(Node::items).unwrap_or_default().iter().filter_map(id_of).collect();
        let near = vc_planner::closest(id, known.iter().map(String::as_str));
        let help = did_you_mean(&near).unwrap_or_else(|| "the Runs space lists the schedules".to_owned());
        return Err(refused("there is no such schedule in the config", &help));
    };
    let whole = root.get(LIST).filter(|list| list.items().len() == 1).unwrap_or(node);
    let change = opened.change(&[opened.document.remove(whole).map_err(internal)?])?;
    Ok(Changes { files: vec![change], titles: vec![format!("Remove schedule {id}")] })
}

/// The change that puts `yaml`, under `comment`, where the item `node` and its comments are.
fn replaced(opened: &Opened<'_>, node: &Node, (yaml, comment): (&str, &str)) -> Result<FileChange, EngineError> {
    let edit = opened.document.remove(node).map_err(internal)?;
    let text = opened.document.text();
    let indent: String = text.get(edit.range.start..).unwrap_or_default().chars().take_while(|c| *c == ' ').collect();
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let new_text = format!("{indent}{comment}\n{}", block(&indent, yaml)).replace('\n', newline);
    opened.change(&[vc_diagnostics::TextEdit { new_text, ..edit }])
}

/// `spec` as the YAML of one list item, every text quoted where YAML needs it.
fn yaml(spec: &ScheduleSpec) -> Result<String, EngineError> {
    if !vc_compiler::is_camel_id(&spec.id) {
        return Err(refused("a schedule's `id` is a camelCase word", "such as `nightlySmoke`"));
    }
    let mut lines = vec![format!("id: {}", spec.id)];
    if let Some(name) = spec.name.as_deref().filter(|name| !name.trim().is_empty()) {
        lines.push(format!("name: {}", write_text(name, false)));
    }
    lines.push(format!("cron: {}", write_text(&spec.cron, false)));
    lines.extend(words("selector", &spec.selector));
    lines.extend(words("profile", &spec.profiles));
    if !spec.notify.is_empty() {
        lines.push("notify:".to_owned());
        for channel in &spec.notify {
            lines.extend(channel_lines(channel)?);
        }
    }
    if spec.runner != ScheduleRunner::VcServer {
        lines.push(format!("runner: {}", spec.runner.name()));
    }
    if !spec.enabled {
        lines.push("enabled: false".to_owned());
    }
    Ok(lines.join("\n"))
}

/// `key` with one text written as text, several as a flow list, none left out.
fn words(key: &str, values: &[String]) -> Option<String> {
    match values {
        [] => None,
        [one] => Some(format!("{key}: {}", write_text(one, false))),
        many => {
            let items: Vec<String> = many.iter().map(|value| write_text(value, true)).collect();
            Some(format!("{key}: [{}]", items.join(", ")))
        }
    }
}

fn channel_lines(channel: &NotifyChannel) -> Result<Vec<String>, EngineError> {
    let help = "a channel kind and its setting keys are words such as `slack` and `webhook`";
    if !is_word(&channel.kind) || channel.settings.iter().any(|(key, _)| !is_word(key)) {
        return Err(refused("a `notify` entry's kind or setting key is not a word", help));
    }
    if channel.settings.is_empty() {
        return Ok(vec![format!("  - {}: {{}}", channel.kind)]);
    }
    let mut lines = vec![format!("  - {}:", channel.kind)];
    lines.extend(channel.settings.iter().map(|(key, value)| format!("      {key}: {}", write_text(value, false))));
    Ok(lines)
}

fn is_word(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// The schedule the config item `node` writes; `None` without an `id`.
fn spec(node: &Node) -> Option<ScheduleSpec> {
    let text = |key: &str| node.get(key).and_then(Node::as_str).map(str::to_owned);
    let runner = text("runner").and_then(|runner| ScheduleRunner::named(&runner)).unwrap_or_default();
    let disabled = node.get("enabled").and_then(Node::as_scalar).is_some_and(|scalar| scalar.text == "false");
    Some(ScheduleSpec {
        id: id_of(node)?,
        name: text("name"),
        cron: text("cron").unwrap_or_default(),
        selector: listed(node, "selector"),
        profiles: listed(node, "profile"),
        notify: node.get("notify").map(Node::items).unwrap_or_default().iter().filter_map(channel).collect(),
        runner,
        enabled: !disabled,
    })
}

fn id_of(node: &Node) -> Option<String> {
    node.get("id").and_then(Node::as_str).map(str::to_owned)
}

/// The texts under `key`: one text, or a list of them.
fn listed(node: &Node, key: &str) -> Vec<String> {
    let Some(value) = node.get(key) else { return Vec::new() };
    let one = value.as_str().map(|text| vec![text.to_owned()]);
    one.unwrap_or_else(|| value.items().iter().filter_map(Node::as_str).map(str::to_owned).collect())
}

fn channel(item: &Node) -> Option<NotifyChannel> {
    let entry = item.entries().first()?;
    let settings = entry.value.entries().iter().filter_map(|setting| {
        let text = setting.value.as_scalar()?.text.clone();
        Some((setting.key.clone(), text))
    });
    Some(NotifyChannel { kind: entry.key.clone(), settings: settings.collect() })
}

#[cfg(test)]
#[path = "schedules_tests.rs"]
mod tests;
