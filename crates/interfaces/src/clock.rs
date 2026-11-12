use std::time::{SystemTime, UNIX_EPOCH};

use vc_model::Date;

const SECONDS_PER_DAY: i64 = 86_400;

/// Tells the date and the time, so the pure compiler is handed today instead of reading the
/// system clock, and a run's start time comes from one place.
///
/// The system adapter reads the system clock; tests use a clock fixed on one day.
pub trait Clock: Send + Sync {
    /// Today's date in UTC.
    fn today(&self) -> Date;

    /// Now, in milliseconds since the Unix epoch: when a run started, for the reports.
    fn now_ms(&self) -> u64;
}

/// The date in UTC of `time`, as a clock that reads the system time tells [`Clock::today`]; a
/// time out of range is the Unix epoch.
#[must_use]
pub fn utc_date(time: SystemTime) -> Date {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_secs()).map_or(i64::MIN, |s| -s),
    };
    Date::from_unix_days(seconds.div_euclid(SECONDS_PER_DAY)).unwrap_or(Date::UNIX_EPOCH)
}

/// `time` in milliseconds since the Unix epoch, as a clock that reads the system time tells
/// [`Clock::now_ms`]; 0 before the epoch.
#[must_use]
pub fn unix_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |after| u64::try_from(after.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use vc_model::Date;

    use super::{unix_millis, utc_date};

    #[test]
    fn dates_are_utc_days_from_the_epoch() {
        assert_eq!(utc_date(UNIX_EPOCH), Date::UNIX_EPOCH);
        let last_second = UNIX_EPOCH + Duration::from_secs(20_760 * 86_400 + 86_399);
        assert_eq!(utc_date(last_second), Date::new(2026, 11, 3).unwrap());
        assert_eq!(utc_date(UNIX_EPOCH - Duration::from_secs(1)), Date::new(1969, 12, 31).unwrap());
    }

    #[test]
    fn milliseconds_count_from_the_epoch_and_stop_at_it() {
        assert_eq!(unix_millis(UNIX_EPOCH + Duration::from_millis(1_500)), 1_500);
        assert_eq!(unix_millis(UNIX_EPOCH - Duration::from_millis(1)), 0);
    }
}
