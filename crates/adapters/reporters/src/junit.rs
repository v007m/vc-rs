//! `JUnit` XML for CI dashboards: one `<testsuite>` per test file, one `<testcase>` per instance.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use vc_interfaces::{Artifact, ReportError, Reporter, RunSummary};
use vc_model::{Event, TestStatus, Totals};

use crate::collect::{Collector, TestRecord};
use crate::names::Names;
use crate::{Color, Destination, ReporterContext, failure, text, xml};

/// The file the reporter writes.
const FILE: &str = "junit.xml";

/// Writes `junit.xml` when the run finishes: failed tests as `<failure>`, blocked ones as
/// `<error>` (they never ran their own checks), skipped ones as `<skipped>`.
#[derive(Debug)]
pub struct JunitReporter {
    names: Names,
    destination: Arc<dyn Destination>,
    collector: Collector,
}

impl JunitReporter {
    /// A reporter saving to the context's destination.
    pub fn new(context: &ReporterContext) -> Self {
        let names = Names::new(context.program(), context.plan());
        Self { names, destination: context.destination(), collector: Collector::new(context.clock()) }
    }

    fn document(&self, run: &RunSummary) -> String {
        let suites: String = self.suites().iter().map(|(file, tests)| self.suite(file, tests)).collect();
        let head = xml::open("testsuites", &counts("vibranium", run.totals, run.duration));
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{head}\n{suites}</testsuites>\n")
    }

    /// The tests by file: files in the order they started, then any others in the order their
    /// first test finished.
    fn suites(&self) -> Vec<(String, Vec<&TestRecord>)> {
        let mut suites: Vec<(String, Vec<&TestRecord>)> =
            self.collector.files.iter().map(|file| (self.names.file(*file).to_owned(), Vec::new())).collect();
        for record in &self.collector.tests {
            let file = self.names.file_of(record.result.test);
            match suites.iter_mut().find(|(name, _)| *name == file) {
                Some((_, tests)) => tests.push(record),
                None => suites.push((file, vec![record])),
            }
        }
        suites
    }

    fn suite(&self, file: &str, tests: &[&TestRecord]) -> String {
        let mut totals = Totals::default();
        let mut time = Duration::ZERO;
        for record in tests {
            totals.add(record.result.status);
            time += record.duration();
        }
        let cases: String = tests.iter().map(|record| self.case(file, record)).collect();
        format!("  {}\n{cases}  </testsuite>\n", xml::open("testsuite", &counts(file, totals, time)))
    }

    fn case(&self, file: &str, record: &TestRecord) -> String {
        let result = &record.result;
        let mut attrs = vec![
            ("name", self.names.instance(result)),
            ("classname", file.to_owned()),
            ("time", xml::seconds(record.duration())),
        ];
        if let Some(at) = self.names.test_location(result.test) {
            attrs.extend([("file", at.path), ("line", at.line.to_string())]);
        }
        let body = self.outcome(record);
        if body.is_empty() {
            return format!("    {}\n", xml::element("testcase", &attrs, None));
        }
        format!("    {}\n{body}    </testcase>\n", xml::open("testcase", &attrs))
    }

    /// What a test case holds besides its name: the failure, error, skip or flaky note.
    fn outcome(&self, record: &TestRecord) -> String {
        let result = &record.result;
        let message = result.failure.as_ref().map(|failed| failed.message.clone()).unwrap_or_default();
        let code = result.failure.as_ref().and_then(|failed| failed.code.clone()).unwrap_or_default();
        let detail = || failure::block(&self.names, record, Color::Plain);
        let element = match result.status {
            TestStatus::Passed => return String::new(),
            TestStatus::Failed => xml::element("failure", &[("message", message), ("type", code)], Some(&detail())),
            TestStatus::Blocked => xml::element("error", &[("message", message), ("type", code)], Some(&detail())),
            TestStatus::Skipped => {
                xml::element("skipped", &[("message", result.skip_reason.clone().unwrap_or_default())], None)
            }
            TestStatus::Flaky => {
                let attempts = text::count(usize::try_from(result.attempts).unwrap_or(0), "attempt");
                let note = format!("flaky: passed after {attempts}; the failed attempt: {message}");
                let note = if message.is_empty() { format!("flaky: passed after {attempts}") } else { note };
                xml::element("system-out", &[], Some(&note))
            }
        };
        format!("      {element}\n")
    }
}

#[async_trait]
impl Reporter for JunitReporter {
    async fn on_event(&mut self, event: &Event) -> Result<(), ReportError> {
        self.collector.take(event);
        Ok(())
    }

    async fn finish(&mut self, run: &RunSummary) -> Result<Vec<Artifact>, ReportError> {
        let path = self.destination.save(FILE, self.document(run).as_bytes())?;
        Ok(vec![Artifact::new("junit", path)])
    }
}

/// A suite's attributes: blocked tests count as errors, flaky ones as passed.
fn counts(name: &str, totals: Totals, time: Duration) -> Vec<(&'static str, String)> {
    vec![
        ("name", name.to_owned()),
        ("tests", totals.total().to_string()),
        ("failures", totals.failed.to_string()),
        ("errors", totals.blocked.to_string()),
        ("skipped", totals.skipped.to_string()),
        ("time", xml::seconds(time)),
    ]
}

#[cfg(test)]
#[path = "junit_tests.rs"]
mod tests;
