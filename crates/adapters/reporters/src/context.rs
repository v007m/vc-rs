use std::ffi::OsStr;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vc_model::{Plan, Program};

use crate::Destination;

/// Whether console output is coloured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Color {
    /// No escape codes.
    #[default]
    Plain,
    /// ANSI colours.
    Ansi,
}

impl Color {
    /// Colour for a terminal, unless `NO_COLOR` is set to anything but the empty string
    /// (<https://no-color.org>). `no_color` is the variable's value, `terminal` whether stdout is
    /// a terminal.
    #[must_use]
    pub fn detect(no_color: Option<&OsStr>, terminal: bool) -> Self {
        let disabled = no_color.is_some_and(|value| !value.is_empty());
        if terminal && !disabled { Self::Ansi } else { Self::Plain }
    }

    /// Whether escape codes are written.
    #[must_use]
    pub const fn is_on(self) -> bool {
        matches!(self, Self::Ansi)
    }
}

/// Time since the run started, as events arrive. It only places a test on the run's timeline:
/// a test's duration is the one the executor measured and put on its result (D305).
pub trait Stopwatch: Send + Sync + Debug {
    /// Time since the start.
    fn elapsed(&self) -> Duration;
}

/// A [`Stopwatch`] on the monotonic system clock.
#[derive(Clone, Copy, Debug)]
pub struct SystemStopwatch(Instant);

impl SystemStopwatch {
    /// A stopwatch started now.
    #[must_use]
    pub fn start() -> Self {
        Self(Instant::now())
    }
}

impl Stopwatch for SystemStopwatch {
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
}

/// What every reporter is built from: the run's program and plan, where output goes, colour and
/// the run's stopwatch. Build one per run and create each reporter from it.
#[derive(Clone, Debug)]
pub struct ReporterContext {
    program: Arc<Program>,
    plan: Arc<Plan>,
    destination: Arc<dyn Destination>,
    color: Color,
    stopwatch: Arc<dyn Stopwatch>,
    headline: Option<String>,
}

impl ReporterContext {
    /// A context for a run of `plan` over `program`, writing to `destination`; plain output and a
    /// stopwatch started now.
    #[must_use]
    pub fn new(program: Arc<Program>, plan: Arc<Plan>, destination: Arc<dyn Destination>) -> Self {
        let stopwatch = Arc::new(SystemStopwatch::start());
        Self { program, plan, destination, color: Color::Plain, stopwatch, headline: None }
    }

    /// The same context with console colour `color`.
    #[must_use]
    pub fn color(self, color: Color) -> Self {
        Self { color, ..self }
    }

    /// The same context timing tests with `stopwatch`.
    #[must_use]
    pub fn stopwatch(self, stopwatch: Arc<dyn Stopwatch>) -> Self {
        Self { stopwatch, ..self }
    }

    /// The same context, with the console's first line (the planner's plan line) in place of
    /// the default `3 tests in 1 file`.
    #[must_use]
    pub fn headline(self, headline: impl Into<String>) -> Self {
        Self { headline: Some(headline.into()), ..self }
    }

    pub(crate) fn program(&self) -> Arc<Program> {
        Arc::clone(&self.program)
    }

    pub(crate) fn plan(&self) -> &Plan {
        &self.plan
    }

    pub(crate) fn destination(&self) -> Arc<dyn Destination> {
        Arc::clone(&self.destination)
    }

    pub(crate) const fn color_mode(&self) -> Color {
        self.color
    }

    pub(crate) fn clock(&self) -> Arc<dyn Stopwatch> {
        Arc::clone(&self.stopwatch)
    }

    pub(crate) fn first_line(&self) -> Option<&str> {
        self.headline.as_deref().and_then(|headline| headline.lines().next())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::time::Duration;

    use super::{Color, Stopwatch, SystemStopwatch};

    #[test]
    fn colour_needs_a_terminal_and_no_no_color() {
        let set = Some(OsStr::new("1"));
        let empty = Some(OsStr::new(""));
        assert_eq!(Color::detect(None, true), Color::Ansi);
        assert_eq!(Color::detect(empty, true), Color::Ansi);
        assert_eq!(Color::detect(set, true), Color::Plain);
        assert_eq!(Color::detect(None, false), Color::Plain);
        assert!(Color::Ansi.is_on() && !Color::Plain.is_on());
        assert_eq!(Color::default(), Color::Plain);
    }

    #[test]
    fn the_system_stopwatch_runs_forward() {
        let watch = SystemStopwatch::start();
        let first = watch.elapsed();
        assert!(watch.elapsed() >= first);
        assert!(first < Duration::from_secs(60));
    }
}
