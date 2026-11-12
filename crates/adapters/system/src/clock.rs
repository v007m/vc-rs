use std::time::SystemTime;

use vc_interfaces::Clock;
use vc_model::Date;

/// The system clock: today's date in UTC, and the time in milliseconds since the epoch.
#[derive(Clone, Copy, Debug, Default)]
#[expect(
    clippy::exhaustive_structs,
    reason = "unit adapter built by the engine builder, shared across the workspace; every user is in this repo (D160)"
)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> Date {
        vc_interfaces::utc_date(SystemTime::now())
    }

    fn now_ms(&self) -> u64 {
        vc_interfaces::unix_millis(SystemTime::now())
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::Clock;
    use vc_model::Date;
    use vc_testkit::clock_suite;

    use super::SystemClock;

    #[test]
    fn passes_the_clock_suite_and_reads_a_recent_day() {
        clock_suite(&SystemClock);
        assert!(SystemClock.today() >= Date::new(2026, 1, 1).unwrap());
        assert!(SystemClock.now_ms() > 1_767_225_600_000, "after 2026-01-01");
    }
}
