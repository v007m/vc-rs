//! Which `alerts` rules fire when a run ends, and the notice each sends. Pure: the run's end
//! hands in what happened and the previous run's status, and gets the notices back.

use std::collections::HashMap;
use std::path::PathBuf;

use vc_interfaces::{Notice, NoticeFailure, Row, RunSummary};
use vc_model::{Alert, AlertTrigger, Program, RunStatus, Tag, Test, TestStatus};

/// The order triggers are tried in: a rule sends one notice per run, for the first that fires.
const PRECEDENCE: [AlertTrigger; 5] = [
    AlertTrigger::Failure,
    AlertTrigger::Blocked,
    AlertTrigger::Threshold,
    AlertTrigger::Flaky,
    AlertTrigger::Recovery,
];

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
    /// Whether anything ran: a test, or a load iteration. A run where nothing did never
    /// recovers.
    pub ran: bool,
    /// The teardown blocks that failed: `afterAll in users.crud`, `afterRun` (D304).
    pub teardown: Vec<String>,
    /// Where the run's main report was written.
    pub report: Option<String>,
}

/// What alerts read of how a run ended: each test's result, the load thresholds breached,
/// whether anything ran, and the main report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ending {
    /// Every test's result, in the order they ended.
    pub tests: Vec<Ended>,
    /// The load thresholds breached, as `metric` or `metric of <test>`.
    pub breaches: Vec<String>,
    /// Whether a test or a load iteration ran.
    pub ran: bool,
    /// The teardown blocks that failed, as the console names them (D304).
    pub teardown: Vec<String>,
    /// The first report file written.
    pub report: Option<PathBuf>,
}

/// What alerts watch of a run as its rows are recorded: every test's tags by full ID, its
/// file's included, and each result so far.
#[derive(Clone, Debug, Default)]
pub struct Watch {
    tags: HashMap<String, Vec<Tag>>,
    ended: Vec<Ended>,
}

impl Watch {
    /// Watches a run of `program`'s tests.
    pub fn of(program: &Program) -> Self {
        let file = |test: &Test| program.test_files.get(&test.file).map(|file| file.tags.as_slice());
        let tags = |test: &Test| test.tags.iter().chain(file(test).unwrap_or_default()).cloned().collect();
        let named = |test: &Test| Some((program.str(test.id.sym())?.to_owned(), tags(test)));
        Self { tags: program.tests.values().filter_map(named).collect(), ended: Vec::new() }
    }

    /// Notes the test results among `rows`.
    pub fn see(&mut self, rows: &[Row]) {
        let ended = rows.iter().filter_map(|row| self.ended(row)).collect::<Vec<_>>();
        self.ended.extend(ended);
    }

    /// What it saw, with the run's main report.
    pub fn ending(self, report: Option<PathBuf>) -> Ending {
        let ran = self.ended.iter().any(|test| test.status != TestStatus::Skipped);
        Ending { tests: self.ended, breaches: Vec::new(), ran, teardown: Vec::new(), report }
    }

    /// The test result `row` holds: `None` for any other row, or a test that has not ended. A
    /// row of `examples` is named `<test>:<n>`, counting from 1.
    fn ended(&self, row: &Row) -> Option<Ended> {
        let Row::Test(row) = row else { return None };
        let status = row.status?;
        let test = row.example_row.map_or_else(|| row.test_id.clone(), |n| format!("{}:{}", row.test_id, n + 1));
        let blocked = row.blocked_by.as_ref().map(|by| format!("blocked by {by}"));
        let message = row.message.clone().or(blocked).unwrap_or_default();
        let tags = self.tags.get(&row.test_id).cloned().unwrap_or_default();
        Some(Ended { test, tags, status, message })
    }
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

/// Whether `rule` covers a run under `summary`'s profiles: every run when it names none.
pub fn in_profiles(rule: &Alert, summary: &RunSummary) -> bool {
    rule.profiles.is_empty() || summary.profiles.iter().any(|profile| rule.profiles.contains(profile))
}

fn fire<'r>(rule: &'r Alert, run: &Finished, previous: Option<RunStatus>) -> Option<Fired<'r>> {
    let tests = run.tests.iter().filter(|test| tagged(rule, test)).collect::<Vec<_>>();
    let seen = Seen { tests: &tests, run, previous };
    let trigger =
        PRECEDENCE.into_iter().filter(|trigger| rule.on.contains(trigger)).find(|&trigger| seen.fires(trigger))?;
    let listed = listed(trigger);
    let failures = tests.iter().filter(|test| listed(test.status));
    let failures = failures.map(|test| NoticeFailure::new(test.test.clone(), test.message.clone())).collect();
    let notice = Notice::new(trigger, run.project.clone(), run.summary.clone()).failures(failures);
    let notice = notice.breaches(run.breaches.clone()).teardown(run.teardown.clone());
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

fn blocked(status: TestStatus) -> bool {
    status == TestStatus::Blocked
}

/// The tests a notice for `trigger` lists: only the blocked ones for `blocked`, the failed and
/// blocked ones otherwise (D256).
fn listed(trigger: AlertTrigger) -> fn(TestStatus) -> bool {
    if trigger == AlertTrigger::Blocked { blocked } else { failed }
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
            AlertTrigger::Failure => self.any(failed) || self.torn(),
            AlertTrigger::Blocked => self.any(blocked),
            AlertTrigger::Threshold => !self.run.breaches.is_empty(),
            AlertTrigger::Flaky => self.any(|status| status == TestStatus::Flaky),
            AlertTrigger::Recovery => self.recovered(),
        }
    }

    fn any(&self, status: impl Fn(TestStatus) -> bool) -> bool {
        self.tests.iter().any(|test| status(test.status))
    }

    /// Whether a teardown block failed. Like a breach, it is the run's, whatever the rule's
    /// tags (D308).
    const fn torn(&self) -> bool {
        !self.run.teardown.is_empty()
    }

    /// The previous run failed, and this one ran and finished with nothing failed, no teardown
    /// failed and nothing breached.
    fn recovered(&self) -> bool {
        self.previous == Some(RunStatus::Failed)
            && self.run.ran
            && self.run.summary.status != RunStatus::Cancelled
            && self.run.breaches.is_empty()
            && !self.torn()
            && !self.any(failed)
    }
}

#[cfg(test)]
#[path = "alerts_tests.rs"]
mod tests;
