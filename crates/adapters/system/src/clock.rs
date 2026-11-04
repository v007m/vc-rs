use std::time::{SystemTime, UNIX_EPOCH};

use vc_interfaces::Clock;
use vc_model::Date;

const SECONDS_PER_DAY: i64 = 86_400;

/// The system clock: today's date in UTC.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> Date {
        let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(after) => i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
            Err(before) => i64::try_from(before.duration().as_secs()).map_or(i64::MIN, |s| -s),
        };
        Date::from_unix_days(seconds.div_euclid(SECONDS_PER_DAY)).unwrap_or(Date::UNIX_EPOCH)
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
    }
}
