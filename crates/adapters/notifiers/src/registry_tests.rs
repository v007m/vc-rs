use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use vc_interfaces::{Notice, NoticeFailure, NotifierFactory, NotifyError, RunSummary, TransportError};
use vc_model::{AlertTrigger, RunStatus, Totals};
use vc_testkit::{ChannelCase, FakeTransport, RecordingTransport, notifier_factory_suite, notifier_suite, status};

use super::NotifierRegistry;

/// Each standard channel with settings it takes and settings it refuses.
fn channels() -> [(&'static str, Value, Value); 4] {
    [
        ("webhook", json!({"url": "https://hooks.example/s3cr3t-path"}), json!({"url": "not a url s3cr3t"})),
        (
            "slack",
            json!({"webhook": "https://hooks.slack.example/T0/B0/s3cr3t", "channel": "#qa"}),
            json!({"webhook": "ftp://files.example/s3cr3t"}),
        ),
        ("teams", json!({"webhook": "https://acme.webhook.example/workflows/s3cr3t"}), json!({})),
        ("pagerduty", json!({"routingKey": "r0ut1ng-key"}), json!({"routingKey": "r0ut1ng-key", "severity": "loud"})),
    ]
}

fn accepting() -> NotifierRegistry {
    NotifierRegistry::standard(Arc::new(FakeTransport::scripted(Vec::new())))
}

fn full_notice(trigger: AlertTrigger) -> Notice {
    let totals = Totals { passed: 7, failed: 1, blocked: 1, flaky: 1, skipped: 2, allowed_failures: 0 };
    let summary = RunSummary::new(RunStatus::Failed, totals).timed(1_767_225_600_000, Duration::from_millis(4_210));
    Notice::new(trigger, "shop".to_owned(), summary.profiles(["staging", "eu"]))
        .failures(vec![
            NoticeFailure::new("users.crud#getsUser", "expected 200, got <500> & more"),
            NoticeFailure::new("users.crud#rejectsBadEmail:2", "blocked: `beforeAll` seedUsers failed"),
        ])
        .breaches(vec!["errorRate".to_owned(), "p95 of checkout.flow#paysOrder".to_owned()])
        .report("reports/ctrf.json")
}

/// What `channel` posts for `notice`: the URL, the content type and the JSON body.
async fn posted(channel: &str, settings: &Value, notice: &Notice) -> (String, String, Value) {
    let (recording, recordings) = RecordingTransport::new(Arc::new(FakeTransport::scripted(Vec::new())));
    let registry = NotifierRegistry::standard(Arc::new(recording));
    registry.create(channel, settings).unwrap().notify(notice).await.unwrap();
    let sent = recordings.take();
    assert_eq!(sent.len(), 1, "one post per notice");
    let request = &sent[0].request;
    assert_eq!(request.method, http::Method::POST);
    let content_type = request.headers.get(http::header::CONTENT_TYPE).unwrap().to_str().unwrap().to_owned();
    (request.uri.to_string(), content_type, serde_json::from_slice(&request.body).unwrap())
}

#[tokio::test]
async fn every_standard_channel_meets_the_contract() {
    let registry = accepting();
    assert_eq!(registry.channels(), ["pagerduty", "slack", "teams", "webhook"]);
    assert_eq!(NotifierRegistry::standard_channels(), registry.channels(), "a check knows them without a transport");
    for (channel, settings, bad) in channels() {
        notifier_factory_suite(&registry, &ChannelCase::new(channel, settings.clone(), bad));
        notifier_suite(&mut || registry.create(channel, &settings).unwrap()).await;
    }
    assert_eq!(format!("{registry:?}"), r#"{"pagerduty", "slack", "teams", "webhook"}"#);
}

#[tokio::test]
async fn each_channel_posts_its_golden_payload() {
    for (channel, settings, _) in channels() {
        let (url, content_type, body) = posted(channel, &settings, &full_notice(AlertTrigger::Failure)).await;
        assert_eq!(content_type, "application/json");
        let expected = settings.get("url").or_else(|| settings.get("webhook")).and_then(Value::as_str);
        assert_eq!(url, expected.unwrap_or("https://events.pagerduty.com/v2/enqueue"));
        insta::assert_snapshot!(channel, serde_json::to_string_pretty(&body).unwrap());
    }
}

#[tokio::test]
async fn pagerduty_resolves_on_recovery_under_one_dedup_key() {
    let settings = json!({"routingKey": "r0ut1ng-key", "url": "https://pd.example/v2/enqueue"});
    let (url, _, triggered) = posted("pagerduty", &settings, &full_notice(AlertTrigger::Failure)).await;
    let (_, _, resolved) = posted("pagerduty", &settings, &full_notice(AlertTrigger::Recovery)).await;
    assert_eq!(url, "https://pd.example/v2/enqueue");
    assert_eq!((&triggered["event_action"], &resolved["event_action"]), (&json!("trigger"), &json!("resolve")));
    assert_eq!(triggered["dedup_key"], "shop#staging+eu");
    assert_eq!(triggered["dedup_key"], resolved["dedup_key"]);
    assert_eq!(triggered["payload"]["severity"], "error", "error unless the settings say otherwise");
}

#[tokio::test]
async fn a_sample_is_titled_test_and_pagerduty_resolves_it_at_once_as_info() {
    let notice = full_notice(AlertTrigger::Failure).as_test();
    let (recording, recordings) = RecordingTransport::new(Arc::new(FakeTransport::scripted(Vec::new())));
    let registry = NotifierRegistry::standard(Arc::new(recording));
    let settings = json!({"routingKey": "r0ut1ng-key", "severity": "critical"});
    registry.create("pagerduty", &settings).unwrap().notify(&notice).await.unwrap();
    let bodies: Vec<Value> =
        recordings.take().iter().map(|sent| serde_json::from_slice(&sent.request.body).unwrap()).collect();
    let actions: Vec<&Value> = bodies.iter().map(|body| &body["event_action"]).collect();
    assert_eq!(actions, [&json!("trigger"), &json!("resolve")]);
    assert!(bodies.iter().all(|body| body["dedup_key"] == "shop#test" && body["payload"]["severity"] == "info"));
    assert!(bodies[0]["payload"]["summary"].as_str().is_some_and(|summary| summary.starts_with("[test] shop: ")));
    let (_, _, slack) = posted("slack", &json!({"webhook": "https://hooks.example/h"}), &notice).await;
    assert_eq!(slack["text"], "[test] shop: run failed (staging+eu)");
}

#[tokio::test]
async fn a_message_lists_ten_failures_each_clipped_and_counts_the_rest() {
    let failures = (0..13).map(|n| NoticeFailure::new(format!("t#n{n}"), "x".repeat(300))).collect();
    let notice = full_notice(AlertTrigger::Failure).failures(failures);
    let (_, _, body) = posted("webhook", &json!({"url": "https://hooks.example/h"}), &notice).await;
    let extra = &body["results"]["extra"];
    assert_eq!(extra["failures"].as_array().map(Vec::len), Some(10));
    assert_eq!(extra["moreFailures"], 3);
    assert_eq!(extra["failures"][0]["message"].as_str().map(|text| text.chars().count()), Some(201));
    let (_, _, slack) = posted("slack", &json!({"webhook": "https://hooks.example/h"}), &notice).await;
    assert!(slack.to_string().contains("and 3 more"), "{slack}");
}

#[tokio::test]
async fn a_refused_post_names_the_status_and_a_failed_one_hides_the_url_path() {
    let notice = full_notice(AlertTrigger::Failure);
    let settings = json!({"webhook": "https://hooks.slack.example/T0/B0/s3cr3t"});
    let forbidden = NotifierRegistry::standard(Arc::new(FakeTransport::scripted(vec![Ok(status(403))])));
    let refused = forbidden.create("slack", &settings).unwrap().notify(&notice).await;
    assert_eq!(refused, Err(NotifyError::Send { channel: "slack".into(), reason: "HTTP 403".into() }));
    let failing = FakeTransport::new(|request, _| {
        let reason = format!("reset while posting {}", request.uri.path());
        Err(TransportError::Connect { host: "hooks.slack.example".into(), reason })
    });
    let down = NotifierRegistry::standard(Arc::new(failing));
    let error = down.create("slack", &settings).unwrap().notify(&notice).await.unwrap_err();
    assert_eq!(error.code().to_string(), "V5117");
    let message = error.to_string();
    assert!(message.contains("hooks.slack.example") && !message.contains("s3cr3t"), "{message}");
}

#[test]
fn settings_errors_name_the_key_not_its_value() {
    let registry = accepting();
    let refused = |channel: &str, settings: Value| match registry.create(channel, &settings).err() {
        Some(NotifyError::Settings { reason, .. }) => reason,
        other => format!("not a settings error: {other:?}"),
    };
    assert_eq!(refused("webhook", json!({})), "needs `url`");
    assert_eq!(refused("slack", json!({"webhook": "ftp://x.example/s3cr3t"})), "`webhook` is not an http or https URL");
    assert_eq!(refused("pagerduty", json!({"severity": "error"})), "needs `routingKey`");
    assert_eq!(refused("teams", json!("https://x.example")), "needs a mapping");
}
