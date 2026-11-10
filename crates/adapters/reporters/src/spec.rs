use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use vc_interfaces::{Artifact, ReportError, Reporter, RunSummary};
use vc_model::{ActionId, ActionOutcome, CleanupRecord, Event, Names, OwnerKey, RunStatus, TestStatus, Totals};

use crate::collect::{Collector, TestRecord};
use crate::{Color, Destination, ReporterContext, failure, text};

/// The width test names are padded to, so durations line up.
const NAME_WIDTH: usize = 60;

/// The `spec` console format: a plan line, one line per test as it finishes with a detailed
/// block under each failure, and a summary.
#[derive(Debug)]
pub struct SpecReporter {
    names: Names,
    destination: Arc<dyn Destination>,
    color: Color,
    collector: Collector,
    headline: Option<String>,
}

impl SpecReporter {
    /// A reporter printing to the context's destination.
    pub fn new(context: &ReporterContext) -> Self {
        Self {
            names: Names::new(context.program(), context.plan()),
            destination: context.destination(),
            color: context.color_mode(),
            collector: Collector::new(context.clock()),
            headline: context.first_line().map(str::to_owned),
        }
    }

    fn print(&self, text: &str) -> Result<(), ReportError> {
        if text.is_empty() { Ok(()) } else { self.destination.print(text) }
    }

    fn shown(&self, event: &Event) -> String {
        match event {
            Event::RunStarted { files, instances } => {
                let counts = || format!("{} in {}", text::count(*instances, "test"), text::count(*files, "file"));
                format!("{}\n\n", text::sanitize(&self.headline.clone().unwrap_or_else(counts)))
            }
            Event::TestFinished { .. } => self.collector.tests.last().map(|r| self.test(r)).unwrap_or_default(),
            Event::ActionFinished { action, owner, outcome } if !matches!(owner, OwnerKey::Instance(_)) => {
                self.lifecycle(*action, *owner, outcome)
            }
            Event::CleanupFinished { cleanup } => self.cleanup(cleanup),
            Event::Warned { warning, .. } => {
                let line = format!("{}: {}", warning.code.as_deref().unwrap_or("warning"), warning.message);
                format!("{} {}\n", self.paint("!", Tone::Warn), text::sanitize(&line))
            }
            Event::CleanupsAbandoned { owner, cleanups } => {
                let actions: Vec<String> = cleanups.iter().map(|action| self.names.action(*action)).collect();
                let line = format!("cleanups abandoned for {}: {}", self.names.owner(*owner), actions.join(", "));
                format!("{} {}\n", self.paint("!", Tone::Warn), text::sanitize(&line))
            }
            _ => String::new(),
        }
    }

    /// The test's line, and its failure block when it failed or was blocked.
    fn test(&self, record: &TestRecord) -> String {
        let result = &record.result;
        let (glyph, tone) = mark(result.status);
        let subject = format!("{}{}", self.names.instance(result), suffix(record));
        let time = if record.start.is_some() { text::duration(record.duration()) } else { String::new() };
        let subject = text::sanitize(&subject);
        let mut out = format!("{} {subject:<NAME_WIDTH$} {time:>6}", self.paint(glyph, tone)).trim_end().to_owned();
        out.push('\n');
        if matches!(result.status, TestStatus::Failed | TestStatus::Blocked) {
            out.push_str(&failure::block(&self.names, record, self.color));
        }
        out
    }

    /// A `beforeRun`, `beforeAll`, `afterAll` or `afterRun` action, shown only when it failed.
    fn lifecycle(&self, action: ActionId, owner: OwnerKey, outcome: &ActionOutcome) -> String {
        let Some(failed) = outcome.failure() else { return String::new() };
        let line = format!("{} ({}) failed", self.names.action(action), self.names.owner(owner));
        let detail = failure::failure(&self.names, failed, self.color);
        format!("{} {}\n{}", self.paint("✕", Tone::Fail), text::sanitize(&line), indent(&detail))
    }

    /// A cleanup, shown only when it failed.
    fn cleanup(&self, cleanup: &CleanupRecord) -> String {
        let Some(failed) = cleanup.outcome.failure() else { return String::new() };
        let action = self.names.action(cleanup.action);
        let line = format!("cleanup {action} ({}) failed: {}", self.names.owner(cleanup.owner), failed.message);
        format!("{} {}\n", self.paint("!", Tone::Warn), text::sanitize(&line))
    }

    fn summary(run: &RunSummary) -> String {
        let mut counts = counts(run.totals);
        if run.status == RunStatus::Cancelled {
            counts.push_str(" · cancelled");
        }
        let mut tail = vec![text::duration(run.duration)];
        tail.extend(profiles(&run.profiles));
        let mut out = format!("\n{counts:<NAME_WIDTH$} {}\n", tail.join(" · "));
        if !run.artifacts.is_empty() {
            let _ = writeln!(out, "reports  {}", reports(&run.artifacts));
        }
        if run.totals.failures() > 0 {
            out.push_str("next     vc run --failed\n");
        }
        text::sanitize(&out).into_owned()
    }

    fn paint(&self, text: &str, tone: Tone) -> String {
        if self.color.is_on() { format!("\x1b[{}m{text}\x1b[0m", tone.ansi()) } else { text.to_owned() }
    }
}

#[async_trait]
impl Reporter for SpecReporter {
    async fn on_event(&mut self, event: &Event) -> Result<(), ReportError> {
        self.collector.take(event);
        self.print(&self.shown(event))
    }

    async fn finish(&mut self, run: &RunSummary) -> Result<Vec<Artifact>, ReportError> {
        self.print(&Self::summary(run))?;
        Ok(Vec::new())
    }
}

#[derive(Clone, Copy)]
enum Tone {
    Pass,
    Fail,
    Warn,
    Quiet,
}

impl Tone {
    const fn ansi(self) -> &'static str {
        match self {
            Self::Pass => "32",
            Self::Fail => "1;31",
            Self::Warn => "33",
            Self::Quiet => "2",
        }
    }
}

const fn mark(status: TestStatus) -> (&'static str, Tone) {
    match status {
        TestStatus::Passed => ("✓", Tone::Pass),
        TestStatus::Failed => ("✕", Tone::Fail),
        TestStatus::Blocked => ("⊘", Tone::Warn),
        TestStatus::Flaky => ("~", Tone::Warn),
        TestStatus::Skipped => ("-", Tone::Quiet),
    }
}

/// What follows the name: ` blocked`, ` flaky (2 attempts)`, ` skipped: rewrite pending`.
fn suffix(record: &TestRecord) -> String {
    let result = &record.result;
    match result.status {
        TestStatus::Passed | TestStatus::Failed => String::new(),
        TestStatus::Blocked => " blocked".to_owned(),
        TestStatus::Flaky => {
            format!(" flaky ({})", text::count(usize::try_from(result.attempts).unwrap_or(0), "attempt"))
        }
        TestStatus::Skipped => {
            result.skip_reason.as_ref().map_or_else(|| " skipped".to_owned(), |r| format!(" skipped: {r}"))
        }
    }
}

/// `9 passed · 1 failed · 1 blocked · 1 skipped · 1 flaky`, leaving out zeros after `passed`.
fn counts(totals: Totals) -> String {
    let others =
        [("failed", totals.failed), ("blocked", totals.blocked), ("skipped", totals.skipped), ("flaky", totals.flaky)];
    let shown = others.into_iter().filter(|(_, n)| *n > 0).map(|(name, n)| format!("{n} {name}"));
    std::iter::once(format!("{} passed", totals.passed)).chain(shown).collect::<Vec<_>>().join(" · ")
}

/// `profile dev`, `profiles staging, eu`, or nothing.
fn profiles(profiles: &[String]) -> Option<String> {
    match profiles {
        [] => None,
        [one] => Some(format!("profile {one}")),
        many => Some(format!("profiles {}", many.join(", "))),
    }
}

/// `out/{ctrf.json,junit.xml}` when the files share a folder, else every path.
fn reports(artifacts: &[Artifact]) -> String {
    let paths: Vec<&Path> = artifacts.iter().map(|a| a.path.as_path()).collect();
    let parent = paths.first().and_then(|p| p.parent()).filter(|dir| paths.iter().all(|p| p.parent() == Some(*dir)));
    let names: Option<Vec<String>> =
        paths.iter().map(|p| p.file_name().map(|name| name.to_string_lossy().into_owned())).collect();
    match (parent, names) {
        (Some(dir), Some(names)) if names.len() > 1 => format!("{}/{{{}}}", dir.display(), names.join(",")),
        _ => paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "),
    }
}

fn indent(block: &str) -> String {
    block.lines().fold(String::new(), |mut out, line| {
        let _ = writeln!(out, "  {line}");
        out
    })
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
