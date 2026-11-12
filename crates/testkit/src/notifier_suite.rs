use std::time::Duration;

use serde_json::Value;
use vc_interfaces::{Notice, NoticeFailure, Notifier, NotifierFactory, NotifyError, RunSummary};
use vc_model::{AlertTrigger, RunStatus, Totals};

/// Builds the notifier under test, pointed at a channel that accepts every message.
pub type MakeNotifier<'a> = dyn FnMut() -> Box<dyn Notifier> + Send + 'a;

/// The contract every [`Notifier`] meets (D112): it sends a notice of every trigger, bare or
/// with failures, breaches, profiles and a report.
///
/// # Panics
///
/// When the notifier breaks the contract.
pub async fn notifier_suite(make: &mut MakeNotifier<'_>) {
    let notifier = make();
    for trigger in AlertTrigger::ALL {
        let bare = Notice::new(trigger, "shop".to_owned(), RunSummary::new(RunStatus::Passed, Totals::default()));
        assert_eq!(notifier.notify(&bare).await, Ok(()), "sends a bare {} notice", trigger.name());
        let full = full_notice(trigger);
        assert_eq!(notifier.notify(&full).await, Ok(()), "sends a full {} notice", trigger.name());
    }
}

/// A notice of `trigger` with every part filled in.
fn full_notice(trigger: AlertTrigger) -> Notice {
    let totals = Totals { passed: 7, failed: 1, blocked: 1, flaky: 1, skipped: 2, allowed_failures: 0 };
    let summary = RunSummary::new(RunStatus::Failed, totals).timed(1_767_225_600_000, Duration::from_millis(4_210));
    Notice::new(trigger, "shop".to_owned(), summary.profiles(["staging", "eu"]))
        .failures(vec![
            NoticeFailure::new("users.crud#getsUser", "expected 200, got 500"),
            NoticeFailure::new("users.crud#rejectsBadEmail:2", "blocked: `beforeAll` seedUsers failed"),
        ])
        .breaches(vec!["errorRate".to_owned(), "p95 of checkout.flow#paysOrder".to_owned()])
        .report("reports/ctrf.json")
}

/// One channel [`notifier_factory_suite`] builds: settings it takes, and settings it refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChannelCase {
    /// The channel kind.
    pub channel: String,
    /// Settings the channel is built from.
    pub settings: Value,
    /// Settings the channel refuses.
    pub bad: Value,
}

impl ChannelCase {
    /// The case of `channel`, built from `settings` and refusing `bad`.
    #[must_use]
    pub fn new(channel: &str, settings: Value, bad: Value) -> Self {
        Self { channel: channel.to_owned(), settings, bad }
    }
}

/// The contract every [`NotifierFactory`] meets (D112).
///
/// It builds the case's channel from its settings, refuses it with its bad settings, and refuses a
/// channel it does not know; no error holds a string of either settings, which may be secret.
///
/// # Panics
///
/// When the factory breaks the contract.
pub fn notifier_factory_suite(factory: &dyn NotifierFactory, case: &ChannelCase) {
    let ChannelCase { channel, settings, bad } = case;
    let channel = channel.as_str();
    let channels = factory.channels();
    assert!(channels.is_sorted() && channels.iter().any(|known| known == channel), "lists {channel}: {channels:?}");
    assert!(factory.create(channel, settings).is_ok(), "builds {channel} from {settings}");
    let unknown = factory.create("carrierPigeon", settings).err();
    let known = channels.join(", ");
    assert_eq!(unknown, Some(NotifyError::Unknown { channel: "carrierPigeon".to_owned(), known }));
    let refused = factory.create(channel, bad).err();
    assert!(matches!(&refused, Some(NotifyError::Settings { channel: named, .. }) if named == channel), "{refused:?}");
    if let Some(error) = refused {
        assert_eq!(error.code().to_string(), "V5117", "every notifier failure is V5117");
        assert_holds_none(&error.to_string(), &[settings, bad]);
    }
}

/// Checks `message` holds no string of `values` longer than three characters.
fn assert_holds_none(message: &str, values: &[&Value]) {
    let mut strings = Vec::new();
    for value in values {
        collect_strings(value, &mut strings);
    }
    for text in strings.into_iter().filter(|text| text.len() > 3) {
        assert!(!message.contains(text), "the error never holds a setting's value: {message}");
    }
}

fn collect_strings<'a>(value: &'a Value, strings: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => strings.push(text),
        Value::Array(items) => items.iter().for_each(|item| collect_strings(item, strings)),
        Value::Object(map) => map.values().for_each(|item| collect_strings(item, strings)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}
