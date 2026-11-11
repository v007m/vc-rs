//! `vc check`'s V5117 warnings for an `alerts` channel no notifier builds, or one whose
//! settings its notifier refuses. The compiler stays pure and knows no channel kinds; the
//! engine's notifier registry does.

use std::sync::Arc;

use serde_json::Value;
use vc_adapter_system::SystemEnvironment;
use vc_diagnostics::{Code, Diagnostic};
use vc_interfaces::{NotifierFactory, NotifyError};
use vc_model::{AlertChannel, CelProgram, FileKind, Program, Reference, SourceFile, SourceFileId, SourceFiles, TemplateValue};
use vc_parser::{Entry, Node, Syntax};
use vc_template::{Chance, PipeRegistry, RenderError, Renderer, Resolve};

use crate::Engine;
use crate::explain::did_you_mean;
use crate::notify::Steady;

/// A warning at each `alerts[].notify[]` key of the config that is none of `known`.
pub fn unknown_channels(files: &SourceFiles, known: &[String]) -> Vec<Diagnostic> {
    let entries = channel_entries(files).into_iter().filter(|entry| !known.contains(&entry.key));
    entries.map(|entry| unknown(&entry, known)).collect()
}

/// A warning at each channel of `program` whose settings, written without a template, the
/// notifier `factory` builds for it refuses. Templated settings are checked when the run ends.
pub fn bad_settings(files: &SourceFiles, program: &Program, factory: &dyn NotifierFactory) -> Vec<Diagnostic> {
    let channels = program.config.alerts.iter().flat_map(|alert| &alert.notify);
    let check = |(entry, channel): (Entry, &AlertChannel)| {
        let settings = plain(&channel.settings)?;
        match factory.create(&channel.kind, &settings) {
            Err(error @ NotifyError::Settings { .. }) => Some(refused(&entry, &error)),
            _ => None,
        }
    };
    channel_entries(files).into_iter().zip(channels).filter_map(check).collect()
}

/// The `alerts[].notify[]` entries of the config, in written order: one per channel.
fn channel_entries(files: &SourceFiles) -> Vec<Entry> {
    let Some((id, file)) = config(files) else { return Vec::new() };
    let (root, _) = vc_parser::parse(id, Syntax::of(&file.path), &file.text);
    let alerts = root.as_ref().and_then(|root| root.get("alerts")).map(Node::items).unwrap_or_default();
    let notify = alerts.iter().flat_map(|alert| alert.get("notify").map(Node::items).unwrap_or_default());
    notify.flat_map(Node::entries).cloned().collect()
}

/// `settings` as JSON when no part of them is a template.
fn plain(settings: &TemplateValue) -> Option<Value> {
    Renderer::new(&PipeRegistry::builtin(), &Untemplated).value(settings).ok()
}

fn refused(entry: &Entry, error: &NotifyError) -> Diagnostic {
    let message = format!("{error}; a run cannot send to it");
    let help = "see `vc explain V5117` for each channel's settings";
    Diagnostic::new(Code::V5117, entry.key_span, message).label("settings refused").help(help)
}

/// Resolves no reference, so a templated setting is left to the run.
struct Untemplated;

impl Resolve for Untemplated {
    fn resolve(&self, _reference: &Reference) -> Result<Value, RenderError> {
        Err(RenderError::invalid("filled in when the run ends"))
    }

    fn evaluate(&self, _program: &CelProgram) -> Result<Value, RenderError> {
        Err(RenderError::invalid("filled in when the run ends"))
    }

    fn chance(&self) -> &dyn Chance {
        &Steady
    }
}

impl Engine {
    /// [`bad_settings`] with the engine's notifiers; none when the HTTP client they post
    /// through cannot be built, which a run reports.
    pub(crate) fn channel_settings(&self, program: &Program, files: &SourceFiles) -> Vec<Diagnostic> {
        if program.config.alerts.is_empty() {
            return Vec::new();
        }
        let environment = self.environment.clone().unwrap_or_else(|| Arc::new(SystemEnvironment::capture()));
        let Ok(transport) = self.transport(environment.as_ref()) else { return Vec::new() };
        bad_settings(files, program, self.notifiers(transport).as_ref())
    }
}

/// The project's `vibranium.config.yaml`, with its number.
pub fn config(files: &SourceFiles) -> Option<(SourceFileId, &SourceFile)> {
    let ids = (0..files.len()).filter_map(|index| u32::try_from(index).ok().map(SourceFileId::new));
    ids.filter_map(|id| Some((id, files.get(id)?))).find(|(_, file)| file.kind() == Some(FileKind::Config))
}

fn unknown(entry: &Entry, known: &[String]) -> Diagnostic {
    let near = vc_planner::closest(&entry.key, known.iter().map(String::as_str));
    let help = did_you_mean(&near).unwrap_or_else(|| format!("the alert channels are {}", known.join(", ")));
    let message = format!("no alert channel `{}`; a run cannot send to it", entry.key);
    Diagnostic::new(Code::V5117, entry.key_span, message).label("unknown channel").help(help)
}

#[cfg(test)]
mod tests {
    use vc_model::{ContentHash, SourceFile, SourceFiles, SourcePath};

    use super::unknown_channels;

    fn files(config: &str) -> SourceFiles {
        let mut files = SourceFiles::new();
        let path = SourcePath::new("vibranium.config.yaml").unwrap();
        files.push(SourceFile { path, text: config.to_owned(), hash: ContentHash::new([0; 32]) }).unwrap();
        files
    }

    #[test]
    fn warns_at_each_unknown_channel_with_the_close_ones() {
        let known = ["pagerduty", "slack", "teams", "webhook"].map(str::to_owned);
        let config = concat!(
            "alerts:\n  - notify:\n      - slak: {channel: '#qa'}\n      - webhook: {url: x}\n",
            "  - notify:\n      - email: {to: qa}\n",
        );
        let warnings = unknown_channels(&files(config), &known);
        let shown =
            warnings.iter().map(|warning| (warning.message.as_str(), warning.help.join(""))).collect::<Vec<_>>();
        assert_eq!(
            shown,
            [
                ("no alert channel `slak`; a run cannot send to it", "did you mean `slack`?".to_owned()),
                (
                    "no alert channel `email`; a run cannot send to it",
                    "the alert channels are pagerduty, slack, teams, webhook".to_owned()
                ),
            ]
        );
        assert_eq!(warnings[0].primary.span.start.line, 3, "it points at the key");
        assert!(warnings.iter().all(|warning| !warning.is_error()), "an alert never fails a check");
    }

    #[test]
    fn a_config_without_alerts_has_nothing_to_warn_about() {
        assert_eq!(unknown_channels(&files("syntaxVersion: 2\n"), &["webhook".to_owned()]), []);
        assert_eq!(unknown_channels(&SourceFiles::new(), &[]), []);
    }
}
