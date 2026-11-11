//! A virtual user: picks a test from the mix, runs one instance of it, thinks, and goes again.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;
use vc_executor::{CancelTree, IterationReport, Iterations};
use vc_interfaces::Random;
use vc_model::TestId;

use crate::{Failed, Mix, Sample};

/// What every user of one run shares.
pub struct Crew {
    pub iterations: Arc<dyn Iterations>,
    pub random: Arc<dyn Random>,
    pub mix: Mix,
    /// The think time's bounds, `(min, max)`.
    pub think: (Duration, Duration),
    /// When the run started: second 0 begins here.
    pub start: Instant,
    pub samples: UnboundedSender<Sample>,
    /// Where each failed iteration goes once it is counted.
    pub failed: UnboundedSender<Failed>,
}

impl Crew {
    /// Passes `report` on if it failed; it must be one the run counted.
    fn tell_failed(&self, test: TestId, report: IterationReport) {
        if !report.passed {
            let _gone = self.failed.send((test, report));
        }
    }
}

/// One user's identity and how it is stopped.
pub struct Seat {
    /// The user's number, from 1.
    pub user: u32,
    /// The run's frame: once it is cancelled the user starts no iteration and records none.
    pub run: CancelTree,
    /// The user's own frame, a child of `run`: cancelling it retires the user once its iteration
    /// ends.
    pub retire: CancelTree,
}

/// Runs iterations for `seat` until it is retired, the run is cancelled or the recorder is gone.
///
/// The [`Iterations`] implementation guards its own actions with the run's frame, so a cancelled
/// iteration still runs its cleanups; one that ends after the cancellation is not recorded, and
/// a failed one is passed on only once it is.
///
/// Each iteration is scheduled for the end of the previous one plus the think time, and its
/// latency counts from that scheduled start, so a user that wakes late cannot hide a slow server
/// (coordinated omission).
pub async fn user(crew: Arc<Crew>, seat: Seat) {
    let mut scheduled = Instant::now();
    loop {
        tokio::select! {
            biased;
            () = seat.retire.cancelled() => return,
            () = tokio::time::sleep_until(scheduled) => {}
        }
        let test = crew.mix.pick(crew.random.next_u64());
        let report = crew.iterations.iterate(test, seat.user).await;
        if seat.run.is_cancelled() {
            return;
        }
        let end = Instant::now();
        let second = end.duration_since(crew.start).as_secs();
        let sample = Sample { test, second, latency: end.duration_since(scheduled), failed: !report.passed };
        if crew.samples.send(sample).is_err() {
            return;
        }
        crew.tell_failed(test, report);
        let pause = think(crew.think, crew.random.next_u64());
        scheduled = end.checked_add(pause).unwrap_or(end);
    }
}

/// A think time between `min` and `max`, inclusive, chosen by `random`.
pub fn think((min, max): (Duration, Duration), random: u64) -> Duration {
    let span = max.saturating_sub(min).as_nanos();
    let extra = u128::from(random).checked_rem(span.saturating_add(1)).unwrap_or(0);
    min.saturating_add(Duration::from_nanos(u64::try_from(extra).unwrap_or(u64::MAX)))
}
