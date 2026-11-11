//! `JUnit` XML for CI dashboards: one `<testsuites>` named for the project, one `<testsuite>` per
//! test file, one `<testcase>` per instance.

use vc_interfaces::{Artifact, ReportError};
use vc_model::{TestStatus, Totals};

use crate::view::TestView;
use crate::{Destination, Format, RunSnapshot, text, xml};

/// The file the format writes.
const FILE: &str = "junit.xml";

/// Writes `junit.xml`.
#[derive(Clone, Copy, Debug)]
pub struct JunitFormat;

impl Format for JunitFormat {
    fn render(&self, snapshot: &RunSnapshot, destination: &dyn Destination) -> Result<Vec<Artifact>, ReportError> {
        let path = destination.save(FILE, document(snapshot).as_bytes())?;
        Ok(vec![Artifact::new("junit", path)])
    }
}

fn document(snapshot: &RunSnapshot) -> String {
    let suites: String = suites(snapshot).iter().map(|(file, tests)| suite(file, tests)).collect();
    let run = &snapshot.run;
    let head = xml::open("testsuites", &counts(snapshot.project(), run.totals, run.duration_ms));
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{head}\n{suites}</testsuites>\n")
}

/// The tests by file: files in the order they started, then any file only a test names.
fn suites(snapshot: &RunSnapshot) -> Vec<(&str, Vec<&TestView>)> {
    let mut suites: Vec<(&str, Vec<&TestView>)> =
        snapshot.files.iter().map(|file| (file.as_str(), Vec::new())).collect();
    for view in &snapshot.tests {
        match suites.iter_mut().find(|(name, _)| *name == view.file) {
            Some((_, tests)) => tests.push(view),
            None => suites.push((&view.file, vec![view])),
        }
    }
    suites
}

fn suite(file: &str, tests: &[&TestView]) -> String {
    let mut totals = Totals::default();
    let mut time = 0_u64;
    for view in tests {
        totals.add(view.status);
        time = time.saturating_add(view.duration_ms());
    }
    let cases: String = tests.iter().map(|view| case(file, view)).collect();
    format!("  {}\n{cases}  </testsuite>\n", xml::open("testsuite", &counts(file, totals, time)))
}

fn case(file: &str, view: &TestView) -> String {
    let mut attrs =
        vec![("name", view.label.clone()), ("classname", file.to_owned()), ("time", xml::seconds(view.duration_ms()))];
    if let Some(at) = &view.location {
        attrs.extend([("file", at.path.clone()), ("line", at.line.to_string())]);
    }
    let body = outcome(view);
    if body.is_empty() {
        return format!("    {}\n", xml::element("testcase", &attrs, None));
    }
    format!("    {}\n{body}    </testcase>\n", xml::open("testcase", &attrs))
}

fn outcome(view: &TestView) -> String {
    let message = view.failure.as_ref().map(|failed| failed.message.clone()).unwrap_or_default();
    let code = view.failure.as_ref().and_then(|failed| failed.code.clone()).unwrap_or_default();
    let element = match view.status {
        TestStatus::Passed => return String::new(),
        TestStatus::Failed => xml::element("failure", &[("message", message), ("type", code)], Some(&view.detail)),
        TestStatus::Blocked => xml::element("error", &[("message", message), ("type", code)], Some(&view.detail)),
        TestStatus::Skipped => {
            xml::element("skipped", &[("message", view.skip_reason.clone().unwrap_or_default())], None)
        }
        TestStatus::Flaky => xml::element("system-out", &[], Some(&flaky(view, &message))),
        TestStatus::AllowedFailure => {
            xml::element("skipped", &[("message", allowed(view, &message))], Some(&view.detail))
        }
    };
    format!("      {element}\n")
}

/// `allowed to fail (reason): message`, a skipped case with its failure (D248).
fn allowed(view: &TestView, message: &str) -> String {
    let reason = view.skip_reason.as_ref().map(|reason| format!(" ({reason})")).unwrap_or_default();
    format!("allowed to fail{reason}: {message}")
}

fn flaky(view: &TestView, message: &str) -> String {
    let attempts = text::count(usize::try_from(view.attempts).unwrap_or(0), "attempt");
    if message.is_empty() {
        format!("flaky: passed after {attempts}")
    } else {
        format!("flaky: passed after {attempts}; the failed attempt: {message}")
    }
}

fn counts(name: &str, totals: Totals, millis: u64) -> Vec<(&'static str, String)> {
    vec![
        ("name", name.to_owned()),
        ("tests", totals.total().to_string()),
        ("failures", totals.failed.to_string()),
        ("errors", totals.blocked.to_string()),
        ("skipped", (totals.skipped + totals.allowed_failures).to_string()),
        ("time", xml::seconds(millis)),
    ]
}

#[cfg(test)]
#[path = "junit_tests.rs"]
mod tests;
