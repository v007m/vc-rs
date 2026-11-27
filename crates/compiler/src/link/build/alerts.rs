//! The config's `alerts`, `plugins` and `schedules`, whose `notify` is an alert's.

use vc_model::{Alert, AlertChannel, AlertTrigger, Config, Cron, PluginRef, Schedule, ScheduleRunner, TemplateValue};
use vc_parser::{Entry, Node};

use super::{Build, Built, read};
use crate::link::project;
use crate::link::project::Source;

/// The triggers of an alert that writes no `on`.
const DEFAULT_ON: [AlertTrigger; 1] = [AlertTrigger::Failure];

impl<'a> Build<'_, 'a> {
    /// `config` with the config's `alerts`, `plugins` and `schedules`.
    pub fn alerts_and_plugins(&mut self, source: &Source<'a>, config: Config) -> Built<Config> {
        let alerts = project::list(source.root, "alerts").iter().map(|alert| self::alert(source, alert)).collect();
        let mut plugins = Vec::new();
        for plugin in project::list(source.root, "plugins") {
            let Some(id) = project::text(plugin, "id") else { continue };
            plugins.push(PluginRef {
                id: self.sym(id)?,
                source: read::owned(plugin, "source").unwrap_or_default(),
                config: plugin.get("config").and_then(|config| read::value(source, config)),
            });
        }
        let schedules = project::schedules(source.root).iter().filter_map(|node| schedule(source, node)).collect();
        Ok(Config { alerts, plugins, schedules, ..config })
    }
}

/// One entry of `alerts`.
fn alert(source: &Source<'_>, node: &Node) -> Alert {
    let written = read::texts(node, "on");
    let named = |name: &String| AlertTrigger::ALL.into_iter().find(|trigger| trigger.name() == name);
    let on = if node.get("on").is_some() { written.iter().filter_map(named).collect() } else { DEFAULT_ON.to_vec() };
    Alert {
        on,
        profiles: read::texts(node, "profiles"),
        tags: read::tags(node),
        notify: project::list(node, "notify").iter().flat_map(|entry| channels(source, entry)).collect(),
    }
}

/// One entry of `schedules`; `None` without an `id` or a `cron` that parses, which the
/// checks have reported.
fn schedule(source: &Source<'_>, node: &Node) -> Option<Schedule> {
    let texts = |key: &str| {
        project::one_or_more(node, key).iter().filter_map(|item| item.as_str()).map(str::to_owned).collect()
    };
    Some(Schedule {
        id: read::owned(node, "id")?,
        name: read::owned(node, "name"),
        cron: Cron::parse(project::text(node, "cron")?).ok()?,
        selector: texts("selector"),
        profiles: texts("profile"),
        notify: project::list(node, "notify").iter().flat_map(|entry| channels(source, entry)).collect(),
        runner: project::text(node, "runner").and_then(ScheduleRunner::named).unwrap_or_default(),
        enabled: project::text(node, "enabled").is_none_or(|enabled| !enabled.eq_ignore_ascii_case("false")),
    })
}

/// The channels of one `notify` entry: one per key, in written order.
fn channels(source: &Source<'_>, node: &Node) -> Vec<AlertChannel> {
    let channel = |entry: &Entry| AlertChannel {
        kind: entry.key.clone(),
        settings: read::value(source, &entry.value).unwrap_or(TemplateValue::Null),
    };
    node.entries().iter().map(channel).collect()
}
