//! The units load metrics are measured in, and how a report shows them.

use serde_json::Value;

/// The unit of a load metric: what a report calls it, rounds it to and shows it as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    name: &'static str,
    /// What a shown value is multiplied by: 100 for a share shown as a percentage.
    scale: f64,
    /// What follows a shown value.
    suffix: &'static str,
    /// The decimal places a JSON value keeps.
    places: i32,
}

impl Unit {
    /// Milliseconds, for the latencies.
    pub const MS: Self = Self { name: "ms", scale: 1.0, suffix: " ms", places: 3 };
    /// A share of the iterations, from 0 to 1, shown as a percentage: `errorRate`.
    pub const SHARE: Self = Self { name: "share", scale: 100.0, suffix: "%", places: 4 };
    /// Iterations per second: `rps`.
    pub const PER_SECOND: Self = Self { name: "perSecond", scale: 1.0, suffix: "/s", places: 3 };

    /// The unit's name in JSON: `ms`, `share` or `perSecond`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// `value` rounded to the unit's places, for JSON: `47.711`, not `47.711000000000006`.
    #[must_use]
    pub fn round(self, value: f64) -> f64 {
        let factor = 10_f64.powi(self.places);
        (value * factor).round() / factor
    }

    /// `value` as a report shows it: `45.471 ms`, `100%`, `12.5/s`.
    #[must_use]
    pub fn show(self, value: f64) -> String {
        self.shown(value, Some(PLACES))
    }

    /// A breached `actual` next to its threshold's `limit`, both with the same decimals:
    /// three, or up to six where fewer would round a number of the limit or show the actual as
    /// one, or as 0 when it is not; past six, both exactly. So a breach never reads as a pass:
    /// `31.25 ms (lt 30 ms)`, `30.0004 ms (lte 30 ms)`, `1.0006 ms (lte 1.0005 ms)`.
    #[must_use]
    pub fn breach(self, actual: f64, limit: &Value) -> String {
        let value = actual * self.scale;
        let limits: Vec<f64> = numbers(limit).iter().map(|limit| limit * self.scale).collect();
        let places = (PLACES..=MOST_PLACES).find(|&places| fits(value, &limits, places));
        format!("{} ({})", self.shown(actual, places), self.limit_in(limit, places))
    }

    /// A threshold's limit as a report shows it, each number in the unit: `lt 30 ms`,
    /// `lt 1%`, `gte 20/s, lt 50/s`.
    #[must_use]
    pub fn limit(self, limit: &Value) -> String {
        self.limit_in(limit, Some(PLACES))
    }

    /// `limit` as [`Self::limit`] shows it, each number as [`Self::shown`] shows it.
    fn limit_in(self, limit: &Value, places: Option<usize>) -> String {
        let each = |limit: &Value| self.limit_in(limit, places);
        match limit {
            Value::Number(number) => {
                number.as_f64().map_or_else(|| number.to_string(), |value| self.shown(value, places))
            }
            Value::Array(items) => items.iter().map(each).collect::<Vec<_>>().join(", "),
            Value::Object(fields) => {
                fields.iter().map(|(name, arg)| format!("{name} {}", each(arg))).collect::<Vec<_>>().join(", ")
            }
            other => other.to_string(),
        }
    }

    /// `value` in the unit with at most `places` decimals, or exactly with `None`.
    fn shown(self, value: f64, places: Option<usize>) -> String {
        let value = value * self.scale;
        let text = places.map_or_else(|| value.to_string(), |places| number(value, places));
        format!("{text}{}", self.suffix)
    }
}

/// Whether `places` decimals show each of `limits` unrounded, and `value` as neither a limit
/// it is not nor 0 when it is not.
fn fits(value: f64, limits: &[f64], places: usize) -> bool {
    let text = number(value, places);
    let unrounded = |limit: &f64| number(*limit, places) == number(*limit, MOST_PLACES);
    let confused = |limit: &f64| number(*limit, places) == text && limit.to_string() != value.to_string();
    !(text == "0" && value.abs() > 0.0) && limits.iter().all(unrounded) && !limits.iter().any(confused)
}

/// The decimals a report shows a value with.
const PLACES: usize = 3;

/// The most decimals a breach is shown with before it is shown exactly.
const MOST_PLACES: usize = 6;

/// `value` with at most `places` decimals and no trailing zeros: `30`, `45.471`.
fn number(value: f64, places: usize) -> String {
    let text = format!("{value:.places$}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Every number in a threshold's limit.
fn numbers(limit: &Value) -> Vec<f64> {
    match limit {
        Value::Number(number) => number.as_f64().into_iter().collect(),
        Value::Array(items) => items.iter().flat_map(numbers).collect(),
        Value::Object(fields) => fields.values().flat_map(numbers).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[path = "unit_tests.rs"]
mod tests;
