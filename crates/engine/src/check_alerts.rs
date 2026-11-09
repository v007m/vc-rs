//! `vc check`'s V5117 warning for an `alerts` channel no notifier builds. The compiler stays
//! pure and knows no channel kinds; the engine's notifier registry does.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{FileKind, SourceFile, SourceFileId, SourceFiles};
use vc_parser::{Entry, Node, Syntax};

use crate::explain::did_you_mean;

/// A warning at each `alerts[].notify[]` key of the config that is none of `known`.
pub fn unknown_channels(files: &SourceFiles, known: &[String]) -> Vec<Diagnostic> {
    let Some((id, file)) = config(files) else { return Vec::new() };
    let (root, _) = vc_parser::parse(id, Syntax::of(&file.path), &file.text);
    let alerts = root.as_ref().and_then(|root| root.get("alerts")).map(Node::items).unwrap_or_default();
    let notify = alerts.iter().flat_map(|alert| alert.get("notify").map(Node::items).unwrap_or_default());
    let entries = notify.flat_map(Node::entries).filter(|entry| !known.contains(&entry.key));
    entries.map(|entry| unknown(entry, known)).collect()
}

/// The project's `vibranium.config.yaml`, with its number.
fn config(files: &SourceFiles) -> Option<(SourceFileId, &SourceFile)> {
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
