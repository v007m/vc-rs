//! The words every channel shares: a headline, the counts and the failures.

use vc_interfaces::{Notice, NoticeFailure};
use vc_model::AlertTrigger;

/// The most failures a message lists; the rest are counted.
const LISTED: usize = 10;

/// The most characters of a failure message a message keeps.
const MESSAGE_CHARS: usize = 200;

/// `shop: run failed (staging)`.
pub fn headline(notice: &Notice) -> String {
    format!("{}: {} ({})", notice.project, what(notice.trigger), notice.profile_key())
}

/// `7 passed, 1 failed, 1 blocked, 1 flaky, 2 skipped`.
pub fn counts(notice: &Notice) -> String {
    let totals = notice.summary.totals;
    format!(
        "{} passed, {} failed, {} blocked, {} flaky, {} skipped",
        totals.passed, totals.failed, totals.blocked, totals.flaky, totals.skipped
    )
}

/// The failures a message lists, each message cut to a readable length.
pub fn listed(notice: &Notice) -> Vec<NoticeFailure> {
    let clip = |failure: &NoticeFailure| NoticeFailure::new(failure.test.clone(), clip(&failure.message));
    notice.failures.iter().take(LISTED).map(clip).collect()
}

/// `and 3 more` when the notice has more failures than a message lists.
pub fn more(notice: &Notice) -> Option<String> {
    let more = notice.failures.len().saturating_sub(LISTED);
    (more > 0).then(|| format!("and {more} more"))
}

/// `Breached: errorRate, p95 of checkout.flow#paysOrder` when a threshold was breached.
pub fn breaches(notice: &Notice) -> Option<String> {
    (!notice.breaches.is_empty()).then(|| format!("Breached: {}", notice.breaches.join(", ")))
}

/// `Report: reports/ctrf.json` when the run wrote one.
pub fn report(notice: &Notice) -> Option<String> {
    notice.report.as_ref().map(|report| format!("Report: {report}"))
}

fn clip(message: &str) -> String {
    match message.char_indices().nth(MESSAGE_CHARS) {
        Some((end, _)) => format!("{}…", &message[..end]),
        None => message.to_owned(),
    }
}

/// What happened, in a few words.
const fn what(trigger: AlertTrigger) -> &'static str {
    match trigger {
        AlertTrigger::Failure => "run failed",
        AlertTrigger::Recovery => "run recovered",
        AlertTrigger::Flaky => "flaky tests",
        AlertTrigger::Threshold => "load thresholds breached",
    }
}
