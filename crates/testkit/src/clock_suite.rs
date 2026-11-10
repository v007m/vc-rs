use vc_interfaces::Clock;
use vc_model::Date;

/// The contract every [`Clock`] meets (D112): today is a real day, and neither the date nor the time
/// ever goes back.
///
/// # Panics
///
/// When `clock` breaks the contract.
pub fn clock_suite(clock: &dyn Clock) {
    let first = clock.today();
    assert_eq!(Date::parse(&first.to_string()), Some(first), "today is a valid day");
    assert!(clock.today() >= first, "the date never goes back");
    let start = clock.now_ms();
    assert!(clock.now_ms() >= start, "the time never goes back");
}
