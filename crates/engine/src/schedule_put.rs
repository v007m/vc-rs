//! A schedule put over one already in the config (D749): each key that changed is set, added or
//! removed in place, one edit at a time, so comments, keys the Runs space does not show, a
//! runner this version does not know and a channel setting that is not text stay as written.

use vc_diagnostics::TextEdit;
use vc_parser::{Document, Node, ParserError, ScalarKind, write_text};

use crate::authoring::{Opened, item};
use crate::edit::{FileChange, internal};
use crate::schedules::{LIST, NotifyChannel, ScheduleSpec, channel, channel_lines, listed, spec as read};

type Made = Result<Option<TextEdit>, ParserError>;

/// A notify list's channels as read from it (`None` where an item is not one), and the ones wanted.
type Channels<'c> = (&'c [Option<NotifyChannel>], &'c [NotifyChannel]);

/// One key's step: the next edit that brings the item `node` closer to the spec, if any.
type Step = fn(&Document, &Node, &ScheduleSpec) -> Made;

const STEPS: [Step; 7] = [name, cron, selector, profile, runner, enabled, notify];

/// A channel the list was read with that its items no longer hold.
const NO_CHANNEL: ParserError = ParserError::WrongNode { expected: "a notify channel", found: "nothing" };

/// More edits than one schedule needs: the guard against a step that never settles.
const MOST_EDITS: usize = 64;

/// The change that makes the config's schedule with `spec`'s `id` read back as `spec`.
pub fn patched(opened: &Opened<'_>, spec: &ScheduleSpec) -> Result<FileChange, crate::EngineError> {
    let mut document = opened.document.clone();
    let mut edits = 0;
    for step in STEPS {
        while let Some(edit) = next(&document, step, spec)? {
            document = document.apply(&[edit]).map_err(internal)?;
            edits += 1;
            if edits > MOST_EDITS {
                return Err(internal("a schedule's edits did not settle"));
            }
        }
    }
    let file = opened.file;
    Ok(FileChange {
        path: file.path.to_string(),
        before: Some(file.text.clone()),
        after: document.text().to_owned(),
        moved_from: None,
    })
}

fn next(document: &Document, step: Step, spec: &ScheduleSpec) -> Result<Option<TextEdit>, crate::EngineError> {
    let node = item(document.root(), LIST, &spec.id).ok_or_else(|| internal("the schedule left the config"))?;
    step(document, node, spec).map_err(internal)
}

fn name(document: &Document, node: &Node, spec: &ScheduleSpec) -> Made {
    text_key(document, node, ("name", spec.name.as_deref().filter(|name| !name.trim().is_empty())))
}

fn cron(document: &Document, node: &Node, spec: &ScheduleSpec) -> Made {
    text_key(document, node, ("cron", Some(&spec.cron)))
}

fn runner(document: &Document, node: &Node, spec: &ScheduleSpec) -> Made {
    let unchanged = read(node).is_some_and(|old| old.runner == spec.runner);
    if unchanged { Ok(None) } else { text_key(document, node, ("runner", Some(spec.runner.name()))) }
}

/// `key` made `value` as text: set in place, added last, or removed when `None`.
fn text_key(document: &Document, node: &Node, (key, value): (&str, Option<&str>)) -> Made {
    match (node.get(key), value) {
        (None, None) => Ok(None),
        (None, Some(value)) => document.insert_entry(node, key, value).map(Some),
        (Some(old), Some(value)) if old.as_str() == Some(value) => Ok(None),
        (Some(old), Some(value)) if old.as_scalar().is_some() => {
            document.set_typed(old, value, ScalarKind::String).map(Some)
        }
        (Some(old), _) => document.remove(old).map(Some),
    }
}

fn enabled(document: &Document, node: &Node, spec: &ScheduleSpec) -> Made {
    let old = node.get("enabled");
    let was = old.and_then(Node::as_scalar).is_none_or(|scalar| scalar.text != "false");
    let value = if spec.enabled { "true" } else { "false" };
    match old {
        _ if was == spec.enabled => Ok(None),
        Some(old) if old.as_scalar().is_some() => document.set_typed(old, value, ScalarKind::Bool).map(Some),
        Some(old) => document.remove(old).map(Some),
        None => document.insert_yaml(node, "enabled", value).map(Some),
    }
}

fn selector(document: &Document, node: &Node, spec: &ScheduleSpec) -> Made {
    words_key(document, node, ("selector", &spec.selector))
}

fn profile(document: &Document, node: &Node, spec: &ScheduleSpec) -> Made {
    words_key(document, node, ("profile", &spec.profiles))
}

/// `key` made `values`: one text or a flow list in place of a value on one line, otherwise the
/// old value removed and the new one added last.
fn words_key(document: &Document, node: &Node, (key, values): (&str, &[String])) -> Made {
    if listed(node, key) == values {
        return Ok(None);
    }
    let Some(old) = node.get(key) else {
        let yaml = match values {
            [one] => write_text(one, false),
            many => many.iter().map(|value| format!("- {}", write_text(value, false))).collect::<Vec<_>>().join("\n"),
        };
        return document.insert_yaml(node, key, &yaml).map(Some);
    };
    let one_line = document.text().get(old.span.byte_range()).is_some_and(|text| !text.contains('\n'));
    if values.is_empty() || !one_line {
        return document.remove(old).map(Some);
    }
    let text = match values {
        [one] => write_text(one, false),
        many => format!("[{}]", many.iter().map(|value| write_text(value, true)).collect::<Vec<_>>().join(", ")),
    };
    Ok(Some(TextEdit::replace(old.span, text)))
}

fn notify(document: &Document, node: &Node, spec: &ScheduleSpec) -> Made {
    let Some(list) = node.get("notify") else {
        return if spec.notify.is_empty() {
            Ok(None)
        } else {
            document.insert_yaml(node, "notify", &lines(&spec.notify)).map(Some)
        };
    };
    let old: Vec<Option<NotifyChannel>> = list.items().iter().map(channel).collect();
    if old.iter().flatten().count() == old.len() && same(old.iter().flatten(), &spec.notify) {
        return Ok(None);
    }
    if spec.notify.is_empty() {
        return document.remove(list).map(Some);
    }
    channels(document, list, (&old, &spec.notify)).or_else(|_| document.remove(list).map(Some))
}

/// The next edit to the channels of the list `list`, `old` as read from it, towards `new`: a
/// channel of the same kind has its settings patched, one found further on is moved up, one
/// too many is removed and one missing is added last (to be moved up by the next edit).
fn channels(document: &Document, list: &Node, (old, new): Channels<'_>) -> Made {
    let differs = |(at, want): (usize, &NotifyChannel)| {
        old.get(at).is_none_or(|have| !same(have.iter(), std::slice::from_ref(want)))
    };
    let Some((at, want)) = new.iter().enumerate().find(|&pair| differs(pair)) else {
        return document.remove(list.items().get(new.len()).ok_or(NO_CHANNEL)?).map(Some);
    };
    let here = old.get(at).and_then(Option::as_ref).zip(list.items().get(at));
    if let Some((_, item)) = here.filter(|(have, _)| have.kind == want.kind) {
        return settings(document, item, want);
    }
    if let Some(from) = old.iter().skip(at + 1).position(|have| same(have.iter(), std::slice::from_ref(want))) {
        return document.move_item(list, at + 1 + from, at).map(Some);
    }
    if old.len() > new.len() {
        return document.remove(list.items().get(at).ok_or(NO_CHANNEL)?).map(Some);
    }
    document.insert_item(list, &item_yaml(want)).map(Some)
}

/// The next edit to the settings of the channel `item`: a text setting set or removed, a
/// setting that is not text removed only when `want` names it, then a missing one added.
fn settings(document: &Document, item: &Node, want: &NotifyChannel) -> Made {
    let map = item.entries().first().map(|entry| &entry.value).ok_or(NO_CHANNEL)?;
    for entry in map.entries() {
        let wanted = want.settings.iter().find(|(key, _)| *key == entry.key).map(|(_, value)| value);
        match (entry.value.as_scalar(), wanted) {
            (None, None) => {}
            (Some(scalar), Some(value)) if scalar.text == *value => {}
            (Some(_), Some(value)) => return document.set_typed(&entry.value, value, ScalarKind::String).map(Some),
            _ => return document.remove(&entry.value).map(Some),
        }
    }
    let missing = want.settings.iter().find(|(key, _)| map.get(key).is_none());
    let Some((key, value)) = missing else { return Ok(None) };
    document.insert_entry(map, key, value).or_else(|_| document.insert_flow_entry(map, key, value)).map(Some)
}

/// Whether the channels read back are the ones wanted, settings in any order.
fn same<'c>(old: impl Iterator<Item = &'c NotifyChannel>, new: &[NotifyChannel]) -> bool {
    let sorted = |channel: &NotifyChannel| {
        let mut settings = channel.settings.clone();
        settings.sort();
        (channel.kind.clone(), settings)
    };
    old.map(sorted).eq(new.iter().map(sorted))
}

/// `channels` as a block list fragment; their kinds and keys were checked as words already.
fn lines(channels: &[NotifyChannel]) -> String {
    let lines = channels.iter().filter_map(|channel| channel_lines(channel).ok()).flatten();
    lines.collect::<Vec<_>>().join("\n")
}

/// `channel` as the YAML of one list item, its dash left out.
fn item_yaml(channel: &NotifyChannel) -> String {
    let lines = lines(std::slice::from_ref(channel));
    lines.lines().map(|line| line.get(2..).unwrap_or_default()).collect::<Vec<_>>().join("\n")
}
