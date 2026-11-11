//! Load engine: stage control, the weighted mix, latency recording and thresholds.
//!
//! The pure parts of `vc load`. A [`StageController`] says how many virtual users run each
//! second; a [`Mix`] picks the test each iteration runs; a [`Recorder`] turns finished
//! iterations into one [`Interval`] per test per second, with [`Latencies`] kept as an
//! `HdrHistogram` and stored in its V2 encoding; [`breaches`] judges the scenario's thresholds on
//! a rolling [`Window`] every second and on the whole run's [`Totals`] at the end.
//!
//! [`run_load`] is the shell around them: a stage loop on a one-second tick, a tokio task per
//! virtual user running the program's tests through the executor's [`vc_executor::Iterations`],
//! and the recording of their [`Sample`]s. It sends each closed [`Second`] to a channel and does
//! no other I/O.

mod error;
mod latency;
mod mix;
mod record;
mod run;
mod stage;
#[cfg(test)]
mod test_support;
mod threshold;
mod unit;
mod user;
mod window;

pub use error::LoadError;
pub use latency::Latencies;
pub use mix::Mix;
pub use record::{Interval, Recorder, Sample, Tally};
pub use run::{Ending, Failed, LoadOutcome, LoadParts, Matching, Second, run_load};
pub use stage::{StageCommand, StageController, StageInput};
pub use threshold::{Breach, aborts, breaches};
pub use unit::Unit;
pub use window::{RATE_SECONDS, Totals, WINDOW_SECONDS, Window};
