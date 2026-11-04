use vc_interfaces::Clock;
use vc_model::Date;

/// A clock stopped on one day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct FixedClock(pub Date);

impl Clock for FixedClock {
    fn today(&self) -> Date {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::Clock;
    use vc_model::Date;

    use super::FixedClock;
    use crate::clock_suite;

    #[test]
    fn passes_the_clock_suite_and_stays_on_its_day() {
        let day = Date::parse("2026-11-03").unwrap();
        let clock = FixedClock(day);
        clock_suite(&clock);
        assert_eq!(clock.today(), day);
    }
}
