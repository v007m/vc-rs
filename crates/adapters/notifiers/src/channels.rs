//! The standard channel kinds: what each posts, and where.

use http::Uri;
use serde_json::{Map, Value, json};
use vc_interfaces::{Notice, NoticeFailure};
use vc_model::AlertTrigger;

use crate::registry::Payload;
use crate::text;

/// Where `PagerDuty`'s Events API v2 takes events.
const PAGERDUTY_EVENTS: &str = "https://events.pagerduty.com/v2/enqueue";

/// The severities `PagerDuty` takes.
const SEVERITIES: [&str; 4] = ["critical", "error", "warning", "info"];

/// `webhook: {url}`: the run's CTRF summary, with the notice under `extra`.
pub struct Webhook;

/// `slack: {webhook, channel}`: a Block Kit message to an incoming webhook.
pub struct Slack;

/// `teams: {webhook}`: an Adaptive Card to a Teams workflow webhook.
pub struct Teams;

/// `pagerduty: {routingKey, severity, url}`: an Events API v2 event that a recovery resolves.
pub struct PagerDuty;

impl Payload for Webhook {
    fn endpoint(&self, settings: &Map<String, Value>) -> Result<Uri, String> {
        url(settings, "url")
    }

    fn body(&self, notice: &Notice, _: &Map<String, Value>) -> Value {
        let totals = notice.summary.totals;
        let start = notice.summary.started_ms;
        let stop = start.saturating_add(u64::try_from(notice.summary.duration.as_millis()).unwrap_or(u64::MAX));
        let summary = json!({
            "tests": totals.total(), "passed": totals.passed + totals.flaky, "failed": totals.failures(),
            "pending": 0, "skipped": totals.skipped, "other": 0, "start": start, "stop": stop,
        });
        let results = json!({"tool": {"name": "vibranium"}, "summary": summary, "extra": details(notice)});
        json!({"reportFormat": "CTRF", "specVersion": "0.2.0", "results": results})
    }
}

impl Payload for Slack {
    fn endpoint(&self, settings: &Map<String, Value>) -> Result<Uri, String> {
        url(settings, "webhook")
    }

    fn body(&self, notice: &Notice, settings: &Map<String, Value>) -> Value {
        let section = |text: &str| json!({"type": "section", "text": {"type": "mrkdwn", "text": slack_escape(text)}});
        let headline = text::headline(notice);
        let mut blocks = vec![json!({"type": "header", "text": {"type": "plain_text", "text": headline}})];
        blocks.push(section(&text::counts(notice)));
        let line = |failure: NoticeFailure| format!("• {}: {}", failure.test, failure.message);
        let lines = text::listed(notice).into_iter().map(line).chain(text::more(notice)).collect::<Vec<_>>();
        blocks.extend((!lines.is_empty()).then(|| section(&lines.join("\n"))));
        blocks.extend(text::breaches(notice).map(|breaches| section(&breaches)));
        let context = |report: String| json!({"type": "context", "elements": [{"type": "mrkdwn", "text": report}]});
        blocks.extend(text::report(notice).map(context));
        let mut message = json!({"text": headline, "blocks": blocks});
        if let Some(channel) = settings.get("channel").and_then(Value::as_str) {
            message["channel"] = json!(channel);
        }
        message
    }
}

impl Payload for Teams {
    fn endpoint(&self, settings: &Map<String, Value>) -> Result<Uri, String> {
        url(settings, "webhook")
    }

    fn body(&self, notice: &Notice, _: &Map<String, Value>) -> Value {
        let block = |text: String| json!({"type": "TextBlock", "text": text, "wrap": true});
        let mut body = vec![json!({"type": "TextBlock", "text": text::headline(notice), "weight": "Bolder",
            "size": "Medium", "wrap": true})];
        body.push(block(text::counts(notice)));
        let facts =
            text::listed(notice).into_iter().map(|failure| json!({"title": failure.test, "value": failure.message}));
        let facts = facts.collect::<Vec<_>>();
        body.extend((!facts.is_empty()).then(|| json!({"type": "FactSet", "facts": facts})));
        body.extend(
            text::more(notice).into_iter().chain(text::breaches(notice)).chain(text::report(notice)).map(block),
        );
        let card = json!({"$schema": "http://adaptivecards.io/schemas/adaptive-card.json", "type": "AdaptiveCard",
            "version": "1.4", "body": body});
        let attachment = json!({"contentType": "application/vnd.microsoft.card.adaptive", "content": card});
        json!({"type": "message", "attachments": [attachment]})
    }
}

impl Payload for PagerDuty {
    fn endpoint(&self, settings: &Map<String, Value>) -> Result<Uri, String> {
        text_at(settings, "routingKey").ok_or_else(|| "needs `routingKey`".to_owned())?;
        let severity = text_at(settings, "severity").unwrap_or("error");
        if !SEVERITIES.contains(&severity) {
            return Err(format!("`severity` is one of {}", SEVERITIES.join(", ")));
        }
        if settings.contains_key("url") { url(settings, "url") } else { Ok(Uri::from_static(PAGERDUTY_EVENTS)) }
    }

    fn body(&self, notice: &Notice, settings: &Map<String, Value>) -> Value {
        let resolve = notice.trigger == AlertTrigger::Recovery;
        let summary = format!("{} — {}", text::headline(notice), text::counts(notice));
        let severity = if notice.test { "info" } else { text_at(settings, "severity").unwrap_or("error") };
        let payload = json!({"summary": summary, "source": notice.project, "severity": severity,
            "custom_details": details(notice)});
        let key = if notice.test { "test".to_owned() } else { notice.profile_key() };
        json!({
            "routing_key": text_at(settings, "routingKey").unwrap_or_default(),
            "event_action": if resolve { "resolve" } else { "trigger" },
            "dedup_key": format!("{}#{key}", notice.project),
            "payload": payload,
        })
    }

    /// A sample's incident is resolved as soon as it opens, with the same `dedup_key` (D257).
    fn then(&self, notice: &Notice, settings: &Map<String, Value>) -> Option<Value> {
        notice.test.then(|| {
            let mut resolve = self.body(notice, settings);
            resolve["event_action"] = json!("resolve");
            resolve
        })
    }
}

/// The notice as plain JSON, for the channels that carry it whole.
fn details(notice: &Notice) -> Value {
    let failures =
        text::listed(notice).into_iter().map(|failure| json!({"test": failure.test, "message": failure.message}));
    json!({
        "trigger": notice.trigger.name(),
        "project": notice.project,
        "profiles": notice.summary.profiles,
        "failures": failures.collect::<Vec<_>>(),
        "moreFailures": notice.failures.len().saturating_sub(text::listed(notice).len()),
        "breaches": notice.breaches,
        "report": notice.report,
        "test": notice.test,
    })
}

/// The setting `key` as text.
fn text_at<'s>(settings: &'s Map<String, Value>, key: &str) -> Option<&'s str> {
    settings.get(key).and_then(Value::as_str)
}

/// The setting `key` as an `http` or `https` URL.
fn url(settings: &Map<String, Value>, key: &str) -> Result<Uri, String> {
    let written = text_at(settings, key).ok_or_else(|| format!("needs `{key}`"))?;
    let uri = Uri::try_from(written).map_err(|_| format!("`{key}` is not a URL"))?;
    let web = matches!(uri.scheme_str(), Some("http" | "https")) && uri.host().is_some();
    if web { Ok(uri) } else { Err(format!("`{key}` is not an http or https URL")) }
}

/// Slack's `mrkdwn` escapes: `&`, `<` and `>`.
fn slack_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
