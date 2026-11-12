//! Sending an alert: the [`Notice`] an alert rule produced at the end of a run, to one channel.

use async_trait::async_trait;
use serde_json::Value;
use vc_diagnostics::Code;
use vc_model::AlertTrigger;

use crate::RunSummary;

/// Sends a [`Notice`] to one channel: a webhook, Slack, Teams, `PagerDuty`.
///
/// Built by a [`NotifierFactory`] from the channel's settings, with secrets already resolved.
/// The engine calls each notifier once per notice at the end of a run; a failure is reported as
/// a V5117 warning and never changes the run's results. Every implementation runs
/// `notifier_suite` (D112).
#[async_trait]
pub trait Notifier: Send + Sync {
    /// Sends `notice`.
    ///
    /// # Errors
    ///
    /// [`NotifyError::Send`] when the channel cannot be reached or refuses the message.
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError>;
}

/// Builds a [`Notifier`] for a channel kind by name (`webhook`, `slack`, …) from its settings.
/// Every implementation runs `notifier_factory_suite` (D112).
pub trait NotifierFactory: Send + Sync {
    /// The channel kinds it builds, sorted.
    fn channels(&self) -> Vec<String>;

    /// The notifier for `channel`, set up from `settings` (the channel's mapping, with
    /// `${secret.*}` already resolved).
    ///
    /// # Errors
    ///
    /// [`NotifyError::Unknown`] for a channel it does not build, [`NotifyError::Settings`] when
    /// a setting is missing or has the wrong type.
    fn create(&self, channel: &str, settings: &Value) -> Result<Box<dyn Notifier>, NotifyError>;
}

/// What one alert rule says about a run: why it fired and the run's outcome, already redacted.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "alert data built by the engine and read by the notifiers; every user is in this repo (D160)"
)]
pub struct Notice {
    /// Why the rule fired.
    pub trigger: AlertTrigger,
    /// The project: the config's `project`, else the folder of `vibranium.config.yaml`.
    pub project: String,
    /// How the run ended, with its active profiles.
    pub summary: RunSummary,
    /// The failed and blocked tests, first ones first.
    pub failures: Vec<NoticeFailure>,
    /// The load thresholds breached, as `metric` or `metric of <testId>`.
    pub breaches: Vec<String>,
    /// The teardown blocks that failed, as the console names them: `afterAll in users.crud`,
    /// `afterRun` (D304).
    pub teardown: Vec<String>,
    /// Where the run's report is: a file path or a URL.
    pub report: Option<String>,
    /// A sample `vc alerts test` sends, not a run's: titled `[test]`, and a `PagerDuty`
    /// incident it opens is resolved at once (D257).
    pub test: bool,
}

impl Notice {
    /// A notice of `trigger` about `project`'s run, with no failures, breaches or report.
    #[must_use]
    pub const fn new(trigger: AlertTrigger, project: String, summary: RunSummary) -> Self {
        Self {
            trigger,
            project,
            summary,
            failures: Vec::new(),
            breaches: Vec::new(),
            teardown: Vec::new(),
            report: None,
            test: false,
        }
    }

    /// The same notice, as a sample `vc alerts test` sends.
    #[must_use]
    pub fn as_test(self) -> Self {
        Self { test: true, ..self }
    }

    /// The same notice listing `failures`.
    #[must_use]
    pub fn failures(self, failures: Vec<NoticeFailure>) -> Self {
        Self { failures, ..self }
    }

    /// The same notice listing the breached `breaches`.
    #[must_use]
    pub fn breaches(self, breaches: Vec<String>) -> Self {
        Self { breaches, ..self }
    }

    /// The same notice naming the failed `teardown` blocks.
    #[must_use]
    pub fn teardown(self, teardown: Vec<String>) -> Self {
        Self { teardown, ..self }
    }

    /// The same notice pointing at `report`.
    #[must_use]
    pub fn report(self, report: impl Into<String>) -> Self {
        Self { report: Some(report.into()), ..self }
    }

    /// The profiles joined by `+`, or `default` when the run named none: the key alerts about
    /// the same profiles share (a `PagerDuty` `dedup_key` is `<project>#<profiles>`).
    #[must_use]
    pub fn profile_key(&self) -> String {
        if self.summary.profiles.is_empty() { "default".to_owned() } else { self.summary.profiles.join("+") }
    }
}

/// One failed or blocked test in a [`Notice`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "alert data built by the engine and read by the notifiers; every user is in this repo (D160)"
)]
pub struct NoticeFailure {
    /// The test's full ID, with its example row: `users.crud#rejectsBadEmail:2`.
    pub test: String,
    /// Why it failed, redacted.
    pub message: String,
}

impl NoticeFailure {
    /// `test` failed with `message`.
    #[must_use]
    pub fn new(test: impl Into<String>, message: impl Into<String>) -> Self {
        Self { test: test.into(), message: message.into() }
    }
}

/// Why an alert could not be sent. It never holds a channel's URL or key.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NotifyError {
    /// No notifier builds this channel kind.
    #[error("unknown alert channel `{channel}`; known channels: {known}")]
    Unknown {
        /// The kind asked for.
        channel: String,
        /// The kinds there are, comma-separated.
        known: String,
    },
    /// A setting is missing or has the wrong type.
    #[error("the `{channel}` alert channel {reason}")]
    Settings {
        /// The channel kind.
        channel: String,
        /// What is wrong, naming the setting but never its value.
        reason: String,
    },
    /// The channel could not be reached or refused the message.
    #[error("the `{channel}` alert was not sent: {reason}")]
    Send {
        /// The channel kind.
        channel: String,
        /// The HTTP status or the network error, without the URL.
        reason: String,
    },
}

impl NotifyError {
    /// The diagnostic code: an alert never changes the run's results, so every cause is the
    /// V5117 warning.
    #[must_use]
    pub const fn code(&self) -> Code {
        Code::V5117
    }
}

#[cfg(test)]
mod tests {
    use vc_diagnostics::Code;
    use vc_model::{AlertTrigger, RunStatus, Totals};

    use super::{Notice, NoticeFailure, NotifyError};
    use crate::RunSummary;

    #[test]
    fn a_notice_is_built_step_by_step_and_keyed_by_its_profiles() {
        let summary = RunSummary::new(RunStatus::Failed, Totals::default());
        let notice = Notice::new(AlertTrigger::Failure, "shop".into(), summary.clone())
            .failures(vec![NoticeFailure::new("users.crud#getsUser", "expected 200, got 500")])
            .breaches(vec!["errorRate".into()])
            .report("reports/ctrf.json");
        assert_eq!(notice.failures[0].test, "users.crud#getsUser");
        assert_eq!(
            (notice.breaches.as_slice(), notice.report.as_deref()),
            (&["errorRate".to_owned()][..], Some("reports/ctrf.json"))
        );
        assert_eq!(notice.profile_key(), "default");
        assert!(!notice.test && notice.as_test().test);
        let profiled = Notice::new(AlertTrigger::Recovery, "shop".into(), summary.profiles(["staging", "eu"]));
        assert_eq!(profiled.profile_key(), "staging+eu");
    }

    #[test]
    fn errors_name_the_channel_and_warn() {
        let unknown = NotifyError::Unknown { channel: "email".into(), known: "slack, webhook".into() };
        assert_eq!(unknown.to_string(), "unknown alert channel `email`; known channels: slack, webhook");
        let sent = NotifyError::Send { channel: "slack".into(), reason: "HTTP 403".into() };
        assert_eq!(sent.to_string(), "the `slack` alert was not sent: HTTP 403");
        assert_eq!(sent.code(), Code::V5117);
    }
}
