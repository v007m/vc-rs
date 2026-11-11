//! RFC 3339 timestamps and Go-style durations, for CEL's `timestamp()` and `duration()` (D153).
//! Both are counts of nanoseconds; the calendar arithmetic is Howard Hinnant's `days_from_civil`.

use std::fmt;

const NANOS: i128 = 1_000_000_000;
const DAY: i128 = 86_400;
/// `0001-01-01T00:00:00Z` and `9999-12-31T23:59:59.999999999Z`, CEL's range for timestamps.
const FIRST: i128 = -62_135_596_800 * NANOS;
const LAST: i128 = 253_402_300_800 * NANOS - 1;
/// Digits of a fraction that count; Go's parser drops the rest too.
const FRACTION_DIGITS: usize = 18;
/// Duration units and their length in nanoseconds; Go spells microseconds three ways.
const UNITS: [(&str, i128); 8] = [
    ("ns", 1),
    ("us", 1_000),
    ("\u{b5}s", 1_000),
    ("\u{3bc}s", 1_000),
    ("ms", 1_000_000),
    ("s", NANOS),
    ("m", 60 * NANOS),
    ("h", 3600 * NANOS),
];

/// An instant: nanoseconds since `1970-01-01T00:00:00Z`, in CEL's range of years 1 to 9999.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(i128);

/// A signed length of time in nanoseconds, in the range of an `i64` as in cel-go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration(i128);

impl Timestamp {
    /// The instant `nanos` after the epoch, when it is in range.
    #[must_use]
    pub const fn from_nanos(nanos: i128) -> Option<Self> {
        if nanos < FIRST || nanos > LAST { None } else { Some(Self(nanos)) }
    }

    /// `self + span`, when the result is in range.
    #[must_use]
    pub const fn plus(self, span: Duration) -> Option<Self> {
        Self::from_nanos(self.0 + span.0)
    }

    /// `self - other`, when it fits a duration.
    #[must_use]
    pub fn since(self, other: Self) -> Option<Duration> {
        Duration::from_nanos(self.0 - other.0)
    }
}

impl Duration {
    /// A duration of `nanos`, when it fits an `i64`.
    #[must_use]
    pub fn from_nanos(nanos: i128) -> Option<Self> {
        i64::try_from(nanos).ok().map(|_| Self(nanos))
    }

    /// `self + other`, when it fits.
    #[must_use]
    pub fn plus(self, other: Self) -> Option<Self> {
        Self::from_nanos(self.0 + other.0)
    }

    /// `-self`.
    #[must_use]
    pub const fn negated(self) -> Self {
        Self(-self.0)
    }
}

/// The instant `ms` milliseconds after the epoch, as RFC 3339 in UTC (`${run.startedAt}`); `None`
/// past the year 9999.
#[must_use]
pub fn rfc3339(ms: u64) -> Option<String> {
    Timestamp::from_nanos(i128::from(ms) * 1_000_000).map(|time| time.to_string())
}

/// Reads an RFC 3339 timestamp: `2024-05-01T12:30:00Z`, `2024-05-01T14:30:00.5+02:00`.
///
/// # Errors
///
/// What is wrong with the text, for a diagnostic.
pub fn parse_timestamp(text: &str) -> Result<Timestamp, String> {
    let mut cursor = Cursor { rest: text.as_bytes() };
    let shape = || format!("`{text}` is not an RFC 3339 timestamp such as `2024-05-01T12:30:00Z`");
    let (year, month, day) = cursor.date().ok_or_else(shape)?;
    let (hour, minute, second) = cursor.time().ok_or_else(shape)?;
    let fraction = cursor.fraction().ok_or_else(shape)?;
    let offset = cursor.offset().ok_or_else(shape)?;
    if !cursor.rest.is_empty() {
        return Err(shape());
    }
    if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return Err(format!("`{text}` is not a calendar date"));
    }
    if hour > 23 || minute > 59 || second > 59 {
        return Err(format!("`{text}` is not a time of day"));
    }
    let seconds = days_from_civil(year, month, day) * DAY + hour * 3600 + minute * 60 + second - offset;
    Timestamp::from_nanos(seconds * NANOS + fraction).ok_or_else(|| format!("`{text}` is outside years 1 to 9999"))
}

/// Reads a Go-style duration: `1h30m`, `1.5s`, `-300ms`, `0`.
///
/// # Errors
///
/// What is wrong with the text, for a diagnostic.
pub fn parse_duration(text: &str) -> Result<Duration, String> {
    let shape = || format!("`{text}` is not a duration such as `1h30m` or `1.5s`");
    let (negative, mut rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if rest == "0" {
        return Ok(Duration(0));
    }
    if rest.is_empty() {
        return Err(shape());
    }
    let mut total = 0_i128;
    while !rest.is_empty() {
        let (nanos, after) = component(rest).ok_or_else(shape)?;
        total = Duration::from_nanos(total + nanos).ok_or_else(|| format!("`{text}` is too long"))?.0;
        rest = after;
    }
    Ok(if negative { Duration(-total) } else { Duration(total) })
}

/// One `<number><unit>` of a duration, in nanoseconds, and the text after it.
fn component(text: &str) -> Option<(i128, &str)> {
    let number_end = text.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(text.len());
    let (number, rest) = text.split_at(number_end);
    let unit_end = rest.find(|c: char| c.is_ascii_digit() || c == '.').unwrap_or(rest.len());
    let (unit, rest) = rest.split_at(unit_end);
    let (_, scale) = UNITS.iter().find(|(name, _)| *name == unit)?;
    Some((scaled(number, *scale)?, rest))
}

/// `number` (digits, an optional `.` and digits) times `scale`.
fn scaled(number: &str, scale: i128) -> Option<i128> {
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if (whole.is_empty() && fraction.is_empty()) || fraction.contains('.') {
        return None;
    }
    let whole: i128 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
    let digits = &fraction[..fraction.len().min(FRACTION_DIGITS)];
    let parts: i128 = if digits.is_empty() { 0 } else { digits.parse().ok()? };
    let denominator = 10_i128.pow(u32::try_from(digits.len()).ok()?);
    whole.checked_mul(scale)?.checked_add(parts * scale / denominator)
}

impl fmt::Display for Timestamp {
    /// RFC 3339 in UTC, the fraction without trailing zeros: `2024-05-01T12:30:00.5Z`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let seconds = self.0.div_euclid(NANOS);
        let (days, second) = (seconds.div_euclid(DAY), seconds.rem_euclid(DAY));
        let (year, month, day) = civil_from_days(days);
        let (hour, minute, second) = (second / 3600, second / 60 % 60, second % 60);
        write!(f, "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")?;
        write!(f, "{}Z", fraction(self.0.rem_euclid(NANOS)))
    }
}

impl fmt::Display for Duration {
    /// Seconds, as CEL writes a duration: `1.5s`, `3600s`, `-0.001s`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let size = self.0.abs();
        write!(f, "{sign}{}{}s", size / NANOS, fraction(size % NANOS))
    }
}

/// `.5` for 500 000 000 nanoseconds; empty for none.
fn fraction(nanos: i128) -> String {
    if nanos == 0 {
        return String::new();
    }
    let digits = format!("{nanos:09}");
    format!(".{}", digits.trim_end_matches('0'))
}

const fn is_leap(year: i128) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

const fn days_in_month(year: i128, month: i128) -> i128 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from `1970-01-01` to a date of the proleptic Gregorian calendar.
const fn days_from_civil(year: i128, month: i128, day: i128) -> i128 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The date `days` after `1970-01-01`.
const fn civil_from_days(days: i128) -> (i128, i128, i128) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 };
    let year = year_of_era + era * 400;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// The unread bytes of a timestamp.
struct Cursor<'a> {
    rest: &'a [u8],
}

impl Cursor<'_> {
    fn date(&mut self) -> Option<(i128, i128, i128)> {
        let year = self.number(4)?;
        self.eat(b"-")?;
        let month = self.number(2)?;
        self.eat(b"-")?;
        Some((year, month, self.number(2)?))
    }

    fn time(&mut self) -> Option<(i128, i128, i128)> {
        self.eat(b"Tt ")?;
        let hour = self.number(2)?;
        self.eat(b":")?;
        let minute = self.number(2)?;
        self.eat(b":")?;
        Some((hour, minute, self.number(2)?))
    }

    /// The nanoseconds of an optional `.123`.
    fn fraction(&mut self) -> Option<i128> {
        if self.eat(b".").is_none() {
            return Some(0);
        }
        let count = self.rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
        if count == 0 || count > 9 {
            return None;
        }
        let digits = self.number(count)?;
        Some(digits * 10_i128.pow(u32::try_from(9 - count).ok()?))
    }

    /// The offset from UTC in seconds: `Z`, `+02:00`, `-05:30`.
    fn offset(&mut self) -> Option<i128> {
        let sign = match self.eat(b"Zz+-")? {
            b'+' => 1,
            b'-' => -1,
            _ => return Some(0),
        };
        let hours = self.number(2)?;
        self.eat(b":")?;
        let minutes = self.number(2)?;
        (hours <= 23 && minutes <= 59).then_some(sign * (hours * 3600 + minutes * 60))
    }

    fn number(&mut self, width: usize) -> Option<i128> {
        let (head, tail) = self.rest.split_at_checked(width)?;
        if !head.iter().all(u8::is_ascii_digit) {
            return None;
        }
        self.rest = tail;
        Some(head.iter().fold(0, |total, digit| total * 10 + i128::from(digit - b'0')))
    }

    /// The next byte, when it is one of `options`.
    fn eat(&mut self, options: &[u8]) -> Option<u8> {
        let (&first, tail) = self.rest.split_first()?;
        options.contains(&first).then(|| {
            self.rest = tail;
            first
        })
    }
}
