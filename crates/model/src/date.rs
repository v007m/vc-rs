use std::fmt;

use serde::{Deserialize, Serialize};

/// A calendar day, as written in a definition file (`2026-11-15`), with no time or zone.
///
/// Ordered by day, so `today > until` says a `skip` has expired. The `Clock` port in
/// `vc-interfaces` gives today's date in UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}

/// Days from 1970-01-01 to 0000-03-01 in the proleptic Gregorian calendar.
const EPOCH_SHIFT: i64 = 719_468;
/// Days in 400 years.
const ERA_DAYS: i64 = 146_097;

impl Date {
    /// 1970-01-01, the day Unix time counts from.
    pub const UNIX_EPOCH: Self = Self { year: 1970, month: 1, day: 1 };

    /// The date `year-month-day`, or `None` when no such day exists (`2026-02-29`) or the
    /// year is outside 1–9999.
    #[must_use]
    pub fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        let valid = (1..=9999).contains(&year) && (1..=12).contains(&month) && day >= 1;
        (valid && day <= days_in_month(year, month)).then_some(Self { year, month, day })
    }

    /// Reads `YYYY-MM-DD` exactly: four, two and two digits.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let number = |range: std::ops::Range<usize>| {
            let part = text.get(range)?;
            part.bytes().all(|b| b.is_ascii_digit()).then(|| part.parse::<u16>().ok()).flatten()
        };
        let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
        Self::new(year, u8::try_from(month).ok()?, u8::try_from(day).ok()?)
    }

    /// The day `days` days after 1970-01-01 (before it when negative), or `None` outside the
    /// years 1–9999.
    #[must_use]
    pub fn from_unix_days(days: i64) -> Option<Self> {
        let shifted = days.checked_add(EPOCH_SHIFT)?;
        let era = shifted.div_euclid(ERA_DAYS);
        let of_era = shifted.rem_euclid(ERA_DAYS);
        let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / (ERA_DAYS - 1)) / 365;
        let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * of_year + 2) / 153;
        let day = of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        Self::new(u16::try_from(year).ok()?, u8::try_from(month).ok()?, u8::try_from(day).ok()?)
    }

    /// The year.
    #[must_use]
    pub const fn year(self) -> u16 {
        self.year
    }

    /// The month, 1–12.
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    /// The day of the month, from 1.
    #[must_use]
    pub const fn day(self) -> u8 {
        self.day
    }
}

const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::Date;

    fn date(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    #[test]
    fn parses_exact_iso_days_only() {
        let day = date("2026-11-15");
        assert_eq!((day.year(), day.month(), day.day()), (2026, 11, 15));
        assert_eq!(day.to_string(), "2026-11-15");
        assert_eq!(date("0001-01-01").to_string(), "0001-01-01");
        for bad in ["2026-1-15", "2026-11-5", "26-11-15", "2026/11/15", "2026-11-15T00", "+026-11-15", "2026-+1-15"] {
            assert_eq!(Date::parse(bad), None, "{bad}");
        }
        for impossible in ["2026-13-01", "2026-00-10", "2026-04-31", "2026-11-00", "0000-01-01", "2026-02-29"] {
            assert_eq!(Date::parse(impossible), None, "{impossible}");
        }
    }

    #[test]
    fn knows_month_lengths_and_leap_years() {
        assert!(Date::new(2024, 2, 29).is_some() && Date::new(2000, 2, 29).is_some());
        assert!(Date::new(1900, 2, 29).is_none() && Date::new(2026, 2, 29).is_none());
        assert!(Date::new(2026, 2, 28).is_some() && Date::new(2026, 1, 31).is_some());
        assert!(Date::new(2026, 6, 30).is_some() && Date::new(2026, 6, 31).is_none());
        assert!(Date::new(2026, 12, 31).is_some() && Date::new(2026, 9, 31).is_none());
        assert!(Date::new(9999, 12, 31).is_some() && Date::new(10_000, 1, 1).is_none());
    }

    #[test]
    fn orders_by_day() {
        assert!(date("2026-11-15") < date("2026-11-16"));
        assert!(date("2026-11-30") < date("2026-12-01"));
        assert!(date("2025-12-31") < date("2026-01-01"));
    }

    #[test]
    fn counts_days_from_the_unix_epoch() {
        assert_eq!(Date::from_unix_days(0), Some(Date::UNIX_EPOCH));
        assert_eq!(Date::UNIX_EPOCH.to_string(), "1970-01-01");
        assert_eq!(Date::from_unix_days(-1), Some(date("1969-12-31")));
        assert_eq!(Date::from_unix_days(59), Some(date("1970-03-01")));
        assert_eq!(Date::from_unix_days(11_016), Some(date("2000-02-29")));
        assert_eq!(Date::from_unix_days(20_760), Some(date("2026-11-03")));
        assert_eq!(Date::from_unix_days(2_932_896), Some(date("9999-12-31")));
        assert_eq!(Date::from_unix_days(2_932_897), None);
        assert_eq!(Date::from_unix_days(i64::MAX), None);
    }
}
