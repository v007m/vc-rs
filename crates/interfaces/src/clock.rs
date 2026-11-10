use vc_model::Date;

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
