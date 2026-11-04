use vc_model::Date;

/// Tells the date, so the pure compiler is handed today instead of reading the system clock.
///
/// The system adapter reads the system clock; tests use a clock fixed on one day.
pub trait Clock: Send + Sync {
    /// Today's date in UTC.
    fn today(&self) -> Date;
}
