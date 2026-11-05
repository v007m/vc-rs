//! The time budget of the running call, which `QuickJS`'s interrupt handler reads.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// When the running call must stop, as nanoseconds since `epoch`; 0 while no call runs.
#[derive(Clone, Debug)]
pub struct Deadline {
    epoch: Instant,
    at: Arc<AtomicU64>,
    fired: Arc<AtomicBool>,
}

impl Deadline {
    /// A deadline with no call running.
    pub fn new() -> Self {
        Self { epoch: Instant::now(), at: Arc::new(AtomicU64::new(0)), fired: Arc::new(AtomicBool::new(false)) }
    }

    /// Starts a call that may run for `budget`.
    pub fn arm(&self, budget: Duration) {
        let budget = u64::try_from(budget.as_nanos()).unwrap_or(u64::MAX);
        self.fired.store(false, Ordering::Relaxed);
        self.at.store(self.now().saturating_add(budget).max(1), Ordering::Relaxed);
    }

    /// Ends the call; whether the interrupt handler stopped it.
    pub fn disarm(&self) -> bool {
        self.at.store(0, Ordering::Relaxed);
        self.fired.swap(false, Ordering::Relaxed)
    }

    /// For the interrupt handler: whether the running call is past its budget, remembering
    /// that it was stopped.
    pub fn interrupt(&self) -> bool {
        let at = self.at.load(Ordering::Relaxed);
        let late = at != 0 && self.now() > at;
        if late {
            self.fired.store(true, Ordering::Relaxed);
        }
        late
    }

    fn now(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::Deadline;

    #[test]
    fn fires_only_while_armed_and_past_its_budget() {
        let deadline = Deadline::new();
        assert!(!deadline.interrupt(), "nothing runs");
        deadline.arm(Duration::from_secs(60));
        assert!(!deadline.interrupt());
        assert!(!deadline.disarm());
        deadline.arm(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(2));
        assert!(deadline.interrupt());
        assert!(deadline.disarm(), "it remembers it fired");
        assert!(!deadline.interrupt() && !deadline.disarm());
    }
}
