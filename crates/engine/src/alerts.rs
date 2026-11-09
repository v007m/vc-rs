//! Which `alerts` rules fire when a run ends, and the notice each sends. Pure: the run's end
//! hands in what happened and the previous run's status, and gets the notices back.
#![cfg_attr(not(test), expect(dead_code, reason = "the run's end sends these from the next commit"))]

use vc_interfaces::{Notice, NoticeFailure, RunSummary};
use vc_model::{Alert, AlertTrigger, RunStatus, Tag, TestStatus};

/// The order triggers are tried in: a rule sends one notice per run, for the first that fires.
const PRECEDENCE: [AlertTrigger; 4] =
    [AlertTrigger::Failure, AlertTrigger::Threshold, AlertTrigger::Flaky, AlertTrigger::Recovery];

/// One test's result, as alerts read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ended {
    /// The full test ID, with its row (`users.crud#rejectsBadEmail:2`).
    pub test: String,
    /// Its tags, the file's included.
    pub tags: Vec<Tag>,
    /// How it ended.
    pub status: TestStatus,
    /// Why it failed or was blocked; empty otherwise.
    pub message: String,
}

/// How a run ended, as alerts read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    /// The project's name.
    pub project: String,
    /// The run's summary.
    pub summary: RunSummary,
    /// Every test's result.
    pub tests: Vec<Ended>,
    /// The load thresholds breached.
    pub breaches: Vec<String>,
    /// Where the run's main report was written.
    pub report: Option<String>,
}

/// A rule that fired, and the notice it sends to each of its channels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fired<'r> {
    /// The rule.
    pub rule: &'r Alert,
    /// Its notice.
    pub notice: Notice,
}

/// The rules that fire for `run`, given the previous run's status, in written order.
pub fn decide<'r>(rules: &'r [Alert], run: &Finished, previous: Option<RunStatus>) -> Vec<Fired<'r>> {
    rules.iter().filter(|rule| in_profiles(rule, &run.summary)).filter_map(|rule| fire(rule, run, previous)).collect()
}

fn in_profiles(rule: &Alert, summary: &RunSummary) -> bool {
    rule.profiles.is_empty() || summary.profiles.iter().any(|profile| rule.profiles.contains(profile))
}

fn fire<'r>(rule: &'r Alert, run: &Finished, previous: Option<RunStatus>) -> Option<Fired<'r>> {
    let tests = run.tests.iter().filter(|test| tagged(rule, test)).collect::<Vec<_>>();
    let seen = Seen { tests: &tests, run, previous };
    let trigger =
        PRECEDENCE.into_iter().filter(|trigger| rule.on.contains(trigger)).find(|&trigger| seen.fires(trigger))?;
    let failures = tests.iter().filter(|test| failed(test.status));
    let failures = failures.map(|test| NoticeFailure::new(test.test.clone(), test.message.clone())).collect();
    let notice = Notice::new(trigger, run.project.clone(), run.summary.clone()).failures(failures);
    let notice = notice.breaches(run.breaches.clone());
    let notice = match &run.report {
        Some(report) => notice.report(report.clone()),
        None => notice,
    };
    Some(Fired { rule, notice })
}

fn tagged(rule: &Alert, test: &Ended) -> bool {
    rule.tags.is_empty() || test.tags.iter().any(|tag| rule.tags.contains(tag))
}

const fn failed(status: TestStatus) -> bool {
    matches!(status, TestStatus::Failed | TestStatus::Blocked)
}

/// What one rule sees of a run: the tests its tags pick, and the run around them.
struct Seen<'s> {
    tests: &'s [&'s Ended],
    run: &'s Finished,
    previous: Option<RunStatus>,
}

impl Seen<'_> {
    fn fires(&self, trigger: AlertTrigger) -> bool {
        match trigger {
            AlertTrigger::Failure => self.any(failed),
            AlertTrigger::Threshold => !self.run.breaches.is_empty(),
            AlertTrigger::Flaky => self.any(|status| status == TestStatus::Flaky),
            AlertTrigger::Recovery => self.recovered(),
        }
    }

    fn any(&self, status: impl Fn(TestStatus) -> bool) -> bool {
        self.tests.iter().any(|test| status(test.status))
    }

    /// The previous run failed, and this one finished with nothing failed and nothing breached.
    fn recovered(&self) -> bool {
        self.previous == Some(RunStatus::Failed)
            && self.run.summary.status != RunStatus::Cancelled
            && self.run.breaches.is_empty()
            && !self.any(failed)
    }
}

#[cfg(test)]
#[path = "alerts_tests.rs"]
mod tests;
