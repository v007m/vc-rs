//! Load engine: stage control, the weighted mix, latency recording and thresholds.
//!
//! The pure parts of `vc load`. A [`StageController`] says how many virtual users run each
//! second; a [`Mix`] picks the test each iteration runs; a [`Recorder`] turns finished
//! iterations into one [`Interval`] per test per second, with [`Latencies`] kept as an
//! `HdrHistogram` and stored in its V2 encoding; [`breaches`] judges the scenario's thresholds on
//! a rolling [`Window`] every second and on the whole run's [`Totals`] at the end. No I/O.

mod error;
mod latency;
mod mix;
mod record;
mod stage;
mod threshold;
mod window;

pub use error::LoadError;
pub use latency::Latencies;
pub use mix::Mix;
pub use record::{Interval, Recorder, Sample, Tally};
pub use stage::{StageCommand, StageController, StageInput};
pub use threshold::{Breach, aborts, breaches};
pub use window::{Totals, WINDOW_SECONDS, Window};
