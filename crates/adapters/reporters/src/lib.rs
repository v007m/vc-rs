//! Reporters and report files: console, CTRF, `JUnit` XML and HTML.
//!
//! Console reporters implement `vc_interfaces::Reporter` and are built by name from the
//! [`ReporterRegistry`] (D43): `spec` and `json` print as the run goes. A reporter gets the
//! program, the plan, where to write ([`Destination`], [`FsDestination`] on disk), whether to
//! colour and a [`Stopwatch`] through one [`ReporterContext`]; [`NoopReporter`] reports nothing.
//!
//! A [`Recorder`] follows the same events and gives the history its rows and, at the end, the
//! run's [`RunSnapshot`] (D167). The report files are [`Format`]s from the [`FormatRegistry`]:
//! `ctrf` writes the canonical `ctrf.json`, `junit` writes `junit.xml` and `html` the HTML
//! report (D169), each rendered from a snapshot, so `vc report` renders them again later. A
//! [`PluginReporter`] hands the run to a reporter plugin named in `reports.formats` (G6).
//! [`mark_run_folder`] marks a run's report folder, and [`prune_run_folders`] drops the marked
//! folders of runs the history no longer keeps.

mod collect;
mod context;
mod ctrf;
mod destination;
mod encode;
mod exchange;
mod failure;
mod folders;
mod format;
mod html;
mod html_data;
mod json;
mod junit;
#[cfg(test)]
mod names_tests;
mod noop;
mod plugin;
mod record;
mod registry;
mod rows;
mod sample;
mod spec;
#[cfg(test)]
mod testing;
mod text;
mod view;
mod xml;

pub use context::{Color, ReporterContext, Stopwatch, SystemStopwatch};
pub use destination::{Destination, FsDestination};
pub use folders::{RUN_MARKER, mark_run_folder, prune_run_folders};
pub use format::{Format, FormatRegistry, HtmlLayout};
pub use noop::NoopReporter;
pub use plugin::PluginReporter;
pub use record::Recorder;
pub use registry::{ReporterFactory, ReporterRegistry};
#[doc(hidden)]
pub use sample::sample_snapshot;
pub use view::{RunFacts, RunSnapshot};
