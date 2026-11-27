//! A schedule's `cron` (D520, D700): five fields, minute, hour, day of the month, month and day
//! of the week, read in UTC as GitHub Actions reads them. Each field is `*`, a number, a range
//! `a-b`, a step `*/n`, `a-b/n` or `a/n` (`a` up to the field's top), or a list of those joined
//! with `,`. A day field is unrestricted only when it is exactly `*` (D747): `*/2` restricts it.
//! When both day fields are restricted, a day matching either fires, as vixie cron has it; a day
//! of the week of `7` is Sunday, as `0` is.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Date;

/// One minute, in milliseconds.
const MINUTE_MS: u64 = 60_000;

/// How many days ahead the next firing is looked for: past any leap day, so `0 0 29 2 *` is found.
const LOOKAHEAD_DAYS: u64 = 8 * 366;

/// Each field's name and the values it takes.
const FIELDS: [(&str, u8, u8); 5] =
    [("minute", 0, 59), ("hour", 0, 23), ("day of the month", 1, 31), ("month", 1, 12), ("day of the week", 0, 7)];

/// A parsed `cron`: the text as written and, for each field, the values it matches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Cron {
    text: String,
    sets: [u64; 5],
    any_day: bool,
    any_weekday: bool,
}

impl Cron {
    /// Parses `text`.
    ///
    /// # Errors
    ///
    /// Which field is wrong and why, as the label of V2317.
    pub fn parse(text: &str) -> Result<Self, String> {
        let parts: Vec<&str> = text.split_whitespace().collect();
        if parts.len() != FIELDS.len() {
            return Err(format!(
                "has {} fields; a cron has 5: minute, hour, day of the month, month, day of the week",
                parts.len()
            ));
        }
        let mut sets = [0; 5];
        for ((set, part), field) in sets.iter_mut().zip(&parts).zip(FIELDS) {
            *set = values(part, field)?;
        }
        if sets[4] & (1 << 7) != 0 {
            sets[4] = (sets[4] | 1) & !(1 << 7);
        }
        let any = |index: usize| parts.get(index).is_some_and(|part| *part == "*");
        Ok(Self { text: parts.join(" "), sets, any_day: any(2), any_weekday: any(4) })
    }

    /// The cron as written, its fields joined by one space.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the minute that starts at `unix_ms` (UTC) is one the cron fires in.
    #[must_use]
    pub fn fires_at(&self, unix_ms: u64) -> bool {
        let minutes = unix_ms / MINUTE_MS;
        let Some((date, weekday)) = day_of(minutes / 1440) else { return false };
        self.on_day(date, weekday) && self.at_time(minutes % 1440)
    }

    /// The start of the first minute after `unix_ms` that the cron fires in, in milliseconds
    /// since the Unix epoch; `None` when no day in the next eight years matches (`0 0 30 2 *`).
    #[must_use]
    pub fn next_after(&self, unix_ms: u64) -> Option<u64> {
        let first = unix_ms / MINUTE_MS + 1;
        let (start_day, start_minute) = (first / 1440, first % 1440);
        for day in start_day..start_day + LOOKAHEAD_DAYS {
            let (date, weekday) = day_of(day)?;
            if !self.on_day(date, weekday) {
                continue;
            }
            let from = if day == start_day { start_minute } else { 0 };
            if let Some(minute) = (from..1440).find(|minute| self.at_time(*minute)) {
                return Some((day * 1440 + minute) * MINUTE_MS);
            }
        }
        None
    }

    const fn on_day(&self, date: Date, weekday: u8) -> bool {
        let month = has(self.sets[3], date.month());
        let by_day = has(self.sets[2], date.day());
        let by_weekday = has(self.sets[4], weekday);
        let day = match (self.any_day, self.any_weekday) {
            (true, true) => true,
            (true, false) => by_weekday,
            (false, true) => by_day,
            (false, false) => by_day || by_weekday,
        };
        month && day
    }

    fn at_time(&self, minute_of_day: u64) -> bool {
        let (hour, minute) = (minute_of_day / 60, minute_of_day % 60);
        let fits = |set: u64, value: u64| u8::try_from(value).is_ok_and(|value| has(set, value));
        fits(self.sets[1], hour) && fits(self.sets[0], minute)
    }
}

impl TryFrom<String> for Cron {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

impl From<Cron> for String {
    fn from(cron: Cron) -> Self {
        cron.text
    }
}

impl fmt::Display for Cron {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

const fn has(set: u64, value: u8) -> bool {
    value < 64 && set & (1 << value) != 0
}

/// The date of the Unix day `day` and its day of the week, `0` for Sunday.
fn day_of(day: u64) -> Option<(Date, u8)> {
    let date = Date::from_unix_days(i64::try_from(day).ok()?)?;
    Some((date, u8::try_from((day + 4) % 7).ok()?))
}

/// The values one field `part` matches, as bits.
fn values(part: &str, (name, low, high): (&str, u8, u8)) -> Result<u64, String> {
    let mut set = 0;
    for item in part.split(',') {
        let (range, step) = item.split_once('/').map_or((item, None), |(range, step)| (range, Some(step)));
        let to_the_top = step.is_some();
        let step = step.map_or(Ok(1), |step| stepped(step, name))?;
        let (from, to) = match range {
            "*" => (low, high),
            _ => match range.split_once('-') {
                Some((from, to)) => (number(from, name)?, number(to, name)?),
                None if to_the_top => (number(range, name)?, high),
                None => (number(range, name)?, number(range, name)?),
            },
        };
        if from < low || to > high {
            return Err(format!("`{item}` in the {name} field is outside {low}-{high}"));
        }
        if from > to {
            return Err(format!("`{item}` in the {name} field runs backwards"));
        }
        set |= (from..=to).step_by(usize::from(step)).fold(0, |bits, value| bits | 1 << value);
    }
    Ok(set)
}

fn number(text: &str, name: &str) -> Result<u8, String> {
    let digits = !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    digits.then(|| text.parse().ok()).flatten().ok_or_else(|| format!("`{text}` in the {name} field is not a number"))
}

/// A step, never `0`, which steps nowhere.
fn stepped(text: &str, name: &str) -> Result<u8, String> {
    let step = number(text, name)?;
    if step == 0 { Err(format!("a step of 0 in the {name} field")) } else { Ok(step) }
}

#[cfg(test)]
#[path = "cron_tests.rs"]
mod tests;
